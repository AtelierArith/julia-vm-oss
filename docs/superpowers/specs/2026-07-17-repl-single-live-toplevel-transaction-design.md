# REPL Single Live Toplevel Transaction Design

**Issue:** #9784  
**Date:** 2026-07-17  
**Status:** Approved direction from the issue acceptance and the owner's instruction to continue

## Goal

Retire the production REPL's remaining fresh full-recompile reconstruction path.
After the first VM is created, every later input must update the same live VM and
compiler snapshot. Runtime values, globals, modules, methods, type identities,
world ranges, and heap objects remain in their owning runtime structures instead
of being copied through `REPLSession` mirrors or converted back into source IR.

Completion deletes every remaining #9784 retirement-list symbol, including
`value_to_init_expr`, `inject_globals`, `extract_assigned_variables`,
`extract_globals_from_vm`, `extract_module_globals_from_vm`,
`restore_module_globals`, `merge_definitions`, `store_definitions`, the typed
definition vectors and indices, `global_types`, `global_struct_names`,
`module_globals`, and `last_struct_heap` transplanting.

## Upstream authority

Julia's REPL applies each transformed input to one module through
`toplevel_eval_with_hooks` and `jl_toplevel_eval_flex`:

- `julia/stdlib/REPL/src/REPL.jl:304-340`
- `julia/base/boot.jl:489`
- `julia/src/toplevel.c:754-1092`

The module binding table, method table/world counter, and heap are the persistent
state. A runtime exception does not replace the module or rebuild its values from
AST literals. `using`, module definitions, method definitions, and ordinary
expressions are different toplevel forms handled by one evaluator, not different
session persistence models.

sjulia should preserve this ownership shape even though it compiles to bytecode
instead of invoking a JIT.

## Current problem

The live append path covers expressions, globals, brand-new generic functions,
and brand-new simple concrete structs. `persistent_delta_eligible` still sends
the following forms to a fresh accumulated compile and a new VM:

- module definitions and non-persistable module surfaces;
- package or explicit `using`/`import`;
- abstract, primitive, enum, and type-alias definitions;
- macros;
- parametric/inner-constructor structs;
- method extension, method redefinition, keyword/parametric functions, and
  lifted helpers;
- opaque runtime `eval`;
- hard-scope `let`/`@testset`;
- any input after an error dropped the held VM.

The new VM requires three parallel reconstructions:

1. prior source definitions are merged back into a `Program`;
2. globals and heap references are converted into init expressions or seeded
   into a transplanted heap;
3. module bindings are mirrored, rewritten into module bodies, and re-run.

Those mechanisms are now the only reason the retirement-list symbols survive.
They also create semantics that upstream Julia does not have: display-form
completeness affects persistence, module bodies can be re-executed, and a
runtime error can force the next input into a reconstructed world.

## Considered approaches

### A. Extend the eligibility whitelist one syntax form at a time

Each currently rejected form would receive another special append path.

This reuses existing code and can land small diffs, but it preserves the wrong
architecture: a growing negative list still decides whether runtime state is
authoritative. Interactions such as a keyword method inside a module loaded by a
relative import would continue to require combined special cases. It cannot
prove that the fresh fallback is unreachable.

### B. Replace AST reconstruction with a typed fresh-VM snapshot

A typed snapshot would copy globals, heap, modules, and definition metadata into
each new VM without calling `value_to_init_expr`.

This removes lossy value-to-source conversion, but it still makes a copied
snapshot the persistence authority. Function indices, `StructRef` type IDs,
closure captures, module identities, dispatch caches, and method worlds would
need cross-program remapping on every fallback. It also retains O(session)
rebuilds and per-feature snapshot completeness.

### C. Apply one validated toplevel delta transaction to the live VM

The compiler produces a typed `ReplToplevelDelta` against the current persistent
snapshot. The VM validates the complete delta, installs definitions and binding
changes in source order, appends the main bytecode, and runs it on the same
frame-0/module/heap/world. This is the selected approach because it matches
upstream's state ownership and makes the retirement list structurally dead.

## Selected architecture

### 1. One runtime authority

After bootstrap, `REPLSession` owns exactly one live `Vm<StableRng>`. The VM owns:

- main/module global bindings and their runtime types;
- the struct heap and object identities;
- module objects and initialized state;
- function/method tables, world ranges, dispatch caches, and backedges;
- runtime-created definitions from `eval`;
- `ans` as an ordinary global binding.

`REPLSession` retains only host configuration, parser/lowering/compiler state,
telemetry, and the live VM. Read-only session APIs delegate to the VM. Display
code borrows the live heap directly; it never stores a second heap snapshot.

### 2. One compiler authority

`ReplPersistentCompile` is the compiler-side image of the same live world. It
owns source-order histories and lowering metadata needed by later inputs, but it
does not own runtime values.

Every successful compile returns both:

```rust
pub struct ReplToplevelDelta {
    pub validation: ReplDeltaValidation,
    pub global_slots: Vec<String>,
    pub type_changes: Vec<ReplTypeChange>,
    pub function_changes: Vec<ReplFunctionChange>,
    pub module_changes: Vec<ReplModuleChange>,
    pub import_changes: Vec<ReplImportChange>,
    pub main_code: Vec<Instr>,
    pub main_source_map: Vec<Option<Span>>,
    pub main_scope_names: HashSet<String>,
    pub next_snapshot: ReplPersistentCompile,
}
```

The envelope is exhaustive. An input is not classified as eligible/ineligible;
supported toplevel forms either compile to a delta or return a typed compile
error. No successful compile is allowed to request a fresh VM fallback.

### 3. Validate, install, execute

`Vm::apply_repl_toplevel_delta` has three phases:

1. **Validate without mutation.** Check code offsets, function and type identity
   anchors, module owner identities, global slot expectations, source-order
   chronology, and every referenced index.
2. **Install toplevel changes.** Grow global slots; append or world-activate
   methods; install new types and module bindings; apply imports; append main
   bytecode. Installation follows source order represented by the delta.
3. **Execute main.** Reset transient stack/output/error state and run from the
   appended entry point while preserving frame 0, heap, modules, caches, and
   world state.

Compile/validation failure leaves both VM and compiler snapshot unchanged.
Once installation begins, definitions have upstream toplevel visibility even if
later runtime code throws. The compiler snapshot advances with the installed
definition world, and assignments performed before an exception remain in the
live VM.

### 4. Runtime error recovery

The current VM is discarded after an error because it can retain non-root call
frames and operand-stack entries. Add a VM-owned recovery operation that:

- unwinds to frame 0;
- clears operand, exception-handler, broadcast, display, and transient I/O state;
- preserves frame-0 bindings, heap, installed definitions, modules, world, and
  caches;
- verifies root-frame and handler-depth invariants before allowing re-entry.

Hard-scope cleanup must restore a shadowed pre-existing global slot instead of
clearing the global binding. The compiler/VM records the shadow as lexical-local
state and removes only that local state at scope exit. Consequently a successful
hard-scope input can also leave the same VM parked.

### 5. Functions and types

The existing `AppendableDelta` is generalized rather than duplicated.

- Method extension and redefinition append a new method world; existing method
  bodies remain addressable for older worlds. Backedge invalidation determines
  which compiled callers must be refreshed.
- Forward-reference repair emits late-bound calls or includes affected callers
  in the delta's recompilation set. It must not rebuild the whole session.
- Keyword, parametric, and lifted helper functions are selected by definition
  identity, not by assuming a contiguous front region.
- Abstract, primitive, enum, parametric, inner-constructor, and concrete type
  changes carry explicit structured identities. Existing type IDs are never
  inferred from short names or vector order.
- Julia-invalid constant/type redefinitions return the upstream-shaped error;
  valid method/module redefinitions create the corresponding new world/binding.

#### Concrete type source-order activation and recovery (#11546)

Concrete type metadata and concrete type visibility are separate states. A
relocatable delta may reserve the complete append-only `StructDefInfo` tail
before main executes so every `NewStruct(type_id, ..)` keeps a stable numeric
identity, but reservation must not publish the corresponding Julia binding.
Today `Vm::install_appended_types` performs both operations eagerly, so
`isdefined(Main, :LaterType)` returns true before execution reaches the later
declaration. That is the wrong result and also makes an errored type delta
impossible to project onto the source prefix that actually ran.

The transaction therefore uses one source-ordered definition-activation
authority shared by functions and types:

- the compiler emits a typed activation marker at each root-source type
  declaration and records the expected ordered definition identities in the
  delta validation envelope;
- reservation appends inert struct metadata at the validated `type_id` tail but
  does not add the name to runtime binding/type lookup, hierarchy, ancestor, or
  constructor visibility tables;
- executing the marker publishes that exact type identity, extends the derived
  hierarchy/ancestor tables, activates its constructor methods, invalidates
  affected dispatch caches, and records the typed activation in order;
- a type declaration and every compiler-generated default/outer constructor
  that belongs to it form one atomic activation group, even when several
  generated functions share the declaration span; recovery never exposes a
  type without its reached constructors or a constructor without its type;
- `isdefined`, reflection, dynamic type lookup, constructor resolution, and
  direct/static construction must all consult the same activation authority;
  compiler source-position checks must prevent a pre-marker `NewStruct` route;
- functions and types may interleave, so recovery validates one ordered typed
  activation trace rather than accepting two unrelated per-kind counts.

On a successful run the full type tail becomes the next compiler snapshot. On a
catchable runtime error, the VM validates the exact reached typed prefix before
mutating session mirrors. Reached concrete types and their constructor methods
remain live. The unreached concrete-type tail is removed from `struct_defs` and
every derived type registry, while the compiler snapshot is projected to the
same reached tail; the old appended main is dead and may retain bytecode operands
for those removed IDs, but no later entry point may target it. This truncation
restores contiguous IDs so the next delta appends at the same VM/compiler
boundary. Recovery fails closed and drops the VM if an unreached type has an
instance, active method, binding, hierarchy edge, or other observable reference.
Dormant generated function bodies may remain solely to preserve function-index
and code-tail alignment, but their method rows and type activation group stay
unpublished exactly like the dormant named-function suffix from PR #11484.

The first concrete-type slice covers brand-new, non-parametric Main-owned
structs already accepted by the LV4 append gate. Abstract, primitive, enum,
parametric, inner-constructor, module-owned, extension, and redefinition forms
remain requirements of the broader identity-based type delta below; this slice
does not count as completion of #9784.

### 6. Modules, imports, packages, and eval

Modules are live objects owned by the VM.

- A new module delta creates and realizes the module once.
- A module redefinition installs a new module binding; it does not rewrite or
  re-run the old module body.
- `using`/`import` changes update the live module's binding/import tables and
  compiler visibility history in source order.
- Package loading compiles its module closure into the same delta envelope and
  runs `__init__` once for that realized module instance.
- Runtime `eval` uses the same compile-and-apply entry point with the target
  module identity. It is not an AST-walker persistence exception.

### 7. Retirement

When all successful post-bootstrap inputs use the transaction path:

- delete the fresh `Vm::new_program` branch from `REPLSession::eval`;
- delete `persistent_delta_eligible` and every whitelist helper;
- make the VM/compiler snapshot fields non-optional after bootstrap;
- delete definition replay vectors/indices and `merge_definitions` /
  `store_definitions`;
- delete `REPLGlobals`, `global_types`, `global_struct_names`, `module_globals`,
  `last_struct_heap`, all extract/restore/inject helpers, and value-to-init
  conversion used only by persistence;
- update audits that currently protect reconstruction so they instead reject
  any reintroduction of the retired symbols.

## Landing sequence

The issue is completed through several independently mergeable PRs, all linked
to #9784. No later `tech-debt` issue starts until the final retirement PR closes
#9784.

### Slice 1: live VM recovery boundary

- Pin upstream parity for assignments/definitions before a runtime error.
- Add `Vm::recover_repl_toplevel_state` and keep the VM after catchable errors.
- Preserve pre-existing globals across hard-scope shadow cleanup and park the VM.
- Remove error/hard-scope as causes of fresh reconstruction.

### Slice 1b: concrete type source-order prefix

- Fix #11546 by separating inert type-ID reservation from Julia-visible type
  activation.
- Record and validate the exact interleaved function/type activation prefix.
- Preserve reached concrete types across a later catchable runtime error and
  remove the unreached type tail transactionally from both VM and compiler
  snapshot.
- Keep same-eval construction after a reached declaration on the live VM.
- Leave #9784 open for the remaining Slice 2–4 retirement work.

### Slice 2: identity-based function/type delta

- Replace contiguous-region extraction with identity-keyed changes.
- Support method extension/redefinition, helpers, all type-definition forms,
  macros, and type aliases.
- Use backedges/late binding for forward references.
- Remove the corresponding `persistent_delta_eligible` branches.

### Slice 3: module/import/eval delta

- Install module and import changes on live module objects.
- Route package loading and runtime `eval` through the same transaction.
- Remove module mirrors and once-initialization reconstruction.

### Slice 4: final state-authority retirement

- Route session inspection/display directly to the VM.
- Remove the fresh/delta branch split and every retirement-list symbol.
- Add a source-only audit that fails if any retired symbol or a post-bootstrap
  `Vm::new_program` path returns.
- Update ADR, status, done, code-audit, and memory documentation.

## Error semantics

- Parse/lowering/compile/validation errors do not mutate live state.
- Toplevel definitions installed before runtime execution remain installed if
  later runtime code throws, matching upstream.
- Global/module mutations executed before a runtime exception remain visible to
  the next input.
- `reset()` drops the live VM and compiler snapshot together, then recreates the
  bootstrap state. It remains observationally equivalent to a new session.
- Host cancellation or resource exhaustion may discard the session only through
  an explicit terminal-session state; it must not silently reconstruct a partial
  world.

## Verification

Each slice adds upstream-oracle session rows and Rust assertions. The final
matrix covers every former `persistent_delta_eligible` rejection class and
asserts that, after bootstrap:

- `last_vm_build_nanos() == Some(0)` for every successful eval;
- the live VM identity/generation does not change;
- values, methods, modules, imports, and type identities match upstream;
- runtime-error partial effects match upstream and the next eval succeeds;
- reset matches a fresh session;
- two independent sessions produce identical golden observations;
- long sessions keep heap/cache bounds.

Required final gates are:

```bash
bash scripts/check_repl_reconstruction_retired.sh
bash scripts/check_ffi_abi_version.sh
cargo fmt --check
bash scripts/run_clippy_lanes.sh default
cargo build --release -p subset_julia_vm --bin sjulia --features repl
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim
timeout 1800 cargo nextest run --release --no-fail-fast
```

The final guarded PR gate must certify the exact current `origin/main` and exact
PR head before regular merge.

## Non-goals

- No JIT is introduced.
- No package/module/type name is special-cased.
- No second serialized persistence snapshot is added.
- No change is made to the C ABI layout.
- AoT package loading is not added; shared compiler/runtime changes must remain
  backend-neutral and preserve the VM as the default runtime.
