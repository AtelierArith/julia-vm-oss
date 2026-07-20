# ADR: REPL Evaluation Model — persistent VM + differential eval (Issue #9199)

*Status: **accepted and landed** — the persistent model is the sole production
REPL model. LV6b partial retirement (Issue #9784, 2026-07-11) removed the
`EvalModel` selector and executable Legacy path after re-pointing the harness to
upstream-reviewed goldens. Persistent full-recompile fallback state remains; see
"Retirement list" below.*
*History: proposed 2026-07-06 (epic decomposition). Slices S1 (this document)
+ S2 (differential harness, `tests/repl_differential_9199_tests.rs`) landed
first with no production changes; S3+/LV1–LV6 implemented the migration.*

This is the decision record for how a `REPLSession` evaluates successive inputs.
It fixes the target shape ("one live `Vm` + differential eval"), the staged
migration, the compatibility rules that hold **during** the migration, and the
exit criteria for retiring the current "accumulate-and-recompile + value→expr
reverse conversion" model.

It complements [SINGLE_THREADED_VM.md](./SINGLE_THREADED_VM.md) (the session is
`!Send`/`!Sync`, one owner thread), [CACHE_ARCHITECTURE.md](./CACHE_ARCHITECTURE.md)
(`PROGRAM_CACHE`), and the world/backedge machinery from Issues #8553/#8554.

---

## Context

### The current model (`subset_julia_vm/src/repl/session.rs`)

`REPLSession` re-enters one live `Vm` for eligible deltas and reconstructs a
fresh VM only for the fail-closed full-recompile fallback. The session still
holds parallel fallback mirrors that must remain synchronized:

`owner_thread`, `seed`, `globals` (`REPLGlobals`, the real `Value`s),
`functions` + `function_index`, `macros` + `macro_index`, `structs` +
`struct_index`, `abstract_types` + `abstract_type_index`, `primitive_types` +
`primitive_type_index`, `enums` + `enum_index`, `modules` + `module_index`,
`usings`, `ans`, `eval_count`, `last_struct_heap`, `global_types`,
`global_struct_names`, `module_globals`, `last_vm_memory_stats`, the persistent
compile bundle, and the optional live VM.

Hand-synchronized groupings that a single logical binding is smeared across:

- **7 `(Vec, HashMap index)` pairs**: functions / macros / structs / abstract
  types / primitive types / enums / modules.
  If an index drifts from its vector, a redefinition either forks a duplicate
  entry or overwrites the wrong one.
- **1 struct global spread over 3 mirrors**: `globals` (the authoritative
  `StructRef`) + `global_types` + `global_struct_names`, reconciled by
  `extract_globals_from_vm`. Fresh fallbacks reconstruct it once from
  `last_struct_heap`; there is no parallel `Literal::Struct` cache.
- `last_struct_heap` indices must stay consistent with every `Value::StructRef(idx)`
  (extraction-order dependent).
- `module_globals` mirrors qualified bindings while extraction may fall back to
  the VM's bare binding name for block-wrapped module constants.

Each `eval` (`session.rs:151-355`) runs this pipeline:

1. **Parse** the input only.
2. **Lower** with the accumulated `usings` + `macros` folded in.
3. `merge_definitions` (`session.rs:358`) — splice **all** prior functions,
   structs, and modules back into this program's IR.
4. `inject_globals` (`session.rs:418`) — reverse-convert every prior global
   **value** back into an AST init literal via `value_to_init_expr`
   (`converters.rs:191`) and prepend it to `main`. Values with no expressible
   form are **value-carried** into the fresh VM (`seed_persisted_globals`,
   Issue #8260).
5. `restore_module_globals` (`session.rs:788`) — module bodies re-run every
   eval, so rewrite each module's `const` initializer to the previously captured
   value to fake persistence (Issue #5296, e.g. `Plots._CURRENT_SERIES`).
6. **Recompile the entire accumulated program** (`compile_with_cache_with_globals`).
7. Build a **brand-new `Vm`**, transplant the prior struct heap, run.
8. After the run, heuristics (`extract_assigned_variables` `converters.rs:670`,
   `extract_globals_from_vm`, `extract_module_globals_from_vm`) guess what to
   persist for next time.

### Why this is a structural bug family, not isolated bugs

- **Every new binding-introducing syntax form must be taught to three places at
  once**: the post-run extraction heuristic (8), the value→expr reverse
  conversion (4), and the compiler type-hint maps `global_types` /
  `global_struct_names`. `begin`-blocks, `let`, `@variables`, tuple assignment
  (#8243), add-assign (#8976) each required chasing all three. The 2026-07-02→04
  fix burst — #9156, #9157, #9172, #9173, #8976, #8977 — plus the (now-closed)
  #9182 and #9193 are all the same follow-the-heuristic omission.
- **The value→expr→value round-trip has a hard expressiveness ceiling.** #8260's
  value-carry exists precisely because some values (self-referential `Rc`
  identity, IO handles, closures over live state) cannot be rebuilt from an
  `Expr`. `globals.rs:166-180` already lists the value variants that are silently
  **not** persisted (`Ref`, `Generator`, `IO`, `BigInt`/`BigFloat`, …).
- **`eval` cost is `O(session length)`.** Every eval re-lowers, re-merges,
  re-hashes, and (on any definition change) recompiles the cumulative program.
  `PROGRAM_CACHE` (`compile/cache.rs:1016`, keyed by the content hash of the
  *merged* program) only helps when nothing changed — a single new definition
  changes the hash, so a growing session pays the lowering/merge/hash cost every
  eval. This directly degrades iOS REPL responsiveness.
- **Module bodies re-run every eval.** This diverges from upstream (a module is
  initialized once, `__init__` runs once, #8994) and forces the `restore_module_globals`
  fakery layer to exist at all (#5296).
- **`reset()` is not `new()`** (`session.rs:859`). Before #9193 it leaked
  `global_types`/`global_struct_names`; even now it does **not** clear
  `last_vm_memory_stats` and does **not** restore the default `InteractiveUtils`
  auto-import that `new()` installs (`session.rs:108-115`) — it only empties
  `usings`. "reset = fresh session" is not structurally guaranteed; it is
  re-derived field by field and drifts.

### Upstream Julia (`./julia`) — the differential model

Upstream never rebuilds the world. Runtime state (method tables, module binding
tables, types) is **one live process-global state**. `jl_toplevel_eval_flex`
(`julia/src/toplevel.c`) lowers and evaluates **only the input expression**; the
result is a side effect on that live state:

- A global variable **is** a binding in the `Main` module. The next eval reads
  the same binding — there is no value→expr→value round-trip. Binding partitions
  carry world ranges to manage redefinition.
- Adding a method bumps the world counter (`jl_method_table_insert`, `src/gf.c:3290`);
  invalidation propagates through backedges to only the affected `CodeInstance`s.
  "Recompile every prior function" never happens. A statement boundary takes
  `ct->world_age = jl_world_counter` (`src/toplevel.c:730-766`); a running
  function is pinned to its entry world (`jl_apply_generic`, `src/gf.c:4559`).
- A module is realized **once** and thereafter referenced as a live object.

Note the script-mode dual, **Issue #9400**: sjulia has no world age across a
program either, so a top-level method redefinition "wins retroactively" (a call
placed *before* the redefinition sees the later method: julia reports world ages
2 then 101; sjulia 101, 101). The REPL's "recompile the accumulated program each
eval" and the script's "one flat program, last definition wins" are the **same
missing-world-age defect** viewed at two granularities. The target model below
fixes both because visibility becomes a world-counter fact, not a recompile
artifact.

---

## Decision — target shape

> **A `REPLSession` owns one long-lived `Vm`. `eval(input)` lowers and compiles
> only the input, applies it as a differential update to that live VM's method
> tables / module binding tables / type registry, and runs only the new
> top-level code on the same VM. Globals live in the VM's module binding table
> and are read directly next eval. Method (re)definitions bump the VM's world
> counter and invalidate only the affected specializations via #8553/#8554
> backedges. Modules are initialized once. `reset()` drops the live VM and makes
> a fresh session.**

Concretely, the target `eval` is:

1. Parse + lower **the input only** (accumulated `usings`/`macros` still feed the
   lowerer, because macro expansion is a compile-time need, not runtime state).
2. **Differentially compile** the input against the live VM: new/changed methods
   are inserted into the live method table (world bump + backedge invalidation);
   new globals get bindings; new struct/module/type definitions register into the
   live registries. Reuse the existing `RuntimeCompileContext` incremental path.
3. **Run only the input's top-level statements** on the live VM. Globals it reads
   resolve against live bindings; globals it writes update those bindings in place.
4. Echo the result exactly as today (display string + artifact), unchanged at the
   C ABI.

The parts that **disappear**: `merge_definitions`, `inject_globals` /
`value_to_init_expr`, `restore_module_globals`, and the post-run
`extract_assigned_variables` / `extract_globals_from_vm` /
`extract_module_globals_from_vm` heuristics. State stops being reconstructed; it
just persists in the VM.

---

## Staged migration

The issue lists four staged steps; this is the refined slice decomposition that
lets each slice ship independently behind the S2 safety net, oldest→newest
dependency order. Each slice keeps the full REPL parity + differential suites
green before merge.

| Slice | Scope | Retires | Depends on |
|---|---|---|---|
| **S1** | This ADR — fix target shape + rules. No production code. | — | — |
| **S2** | Differential harness (`tests/repl_differential_9199_tests.rs`) + migration-seam corpus. Test-only. | — | — |
| **S3** | **Globals primary storage → VM binding table.** Make the live VM's module scope the single home for global values + their types; `REPLSession` reads through it. | `value_to_init_expr`, `inject_globals`, `extract_assigned_variables`, `extract_globals_from_vm`, `global_types`, `global_struct_names`, `struct_instances`. #9193 (reset leak) dies because reset = drop the VM. | S2 green |
| **S4** | **Module once-initialization.** Keep the realized module object live; stop re-running module bodies each eval. | `restore_module_globals`, `module_globals`, the #5296 fakery. | S3 |
| **S5** | **Incremental compile of the input delta.** `eval` compiles only the input against the live VM instead of the merged accumulated program. Makes eval cost independent of session length. | `merge_definitions`, the accumulated-program recompile, the growing `PROGRAM_CACHE` key. | S3, S4 |
| **S6** | **Precise redefinition invalidation.** Method (re)definition bumps the VM world counter and invalidates only affected specializations via #8553/#8554 backedges, instead of relying on "next eval recompiles everything" to make a redefinition visible. Also fixes the script-mode dual #9400. | The last remaining reliance on full recompile for redefinition visibility. | S5; foundation from #8553/#8554 and interned type IDs (#9197) |

Rationale for the ordering: globals (S3) are the largest and most bug-prone
surface and unblock the `reset = new` guarantee immediately; modules (S4) are
self-contained; only once state lives in the VM (S3+S4) can compilation stop
re-materializing it (S5); world-precise invalidation (S6) is the final removal of
"recompile = visibility" and needs the type-identity foundation (#9197) to make
the typemap index reliable.

### Migration status (implementation log)

- **S1 / S2** — merged (this ADR + `tests/repl_differential_9199_tests.rs` +
  `fixtures/repl_differential/migration_seams_9199.toml`).
- **S3 — IMPLEMENTED behind the `EvalModel` selector** (`repl/session.rs`;
  default remains `Legacy`). Scope actually shipped, per the "route the rest
  through Legacy" rule:
  - **Persistent (value-carried, no value→expr round-trip):** the leak-prone
    *simple* globals — scalars, `String`, and callables (`Function` / `Closure` /
    `ComposedFunction`) — gated by `is_persistent_carriable`. This is the family
    the #9182 / #9157 / #8976 fix burst chased through the reverse conversion.
    Mutations a called function makes via `global x = …` are recovered by
    re-reading prior globals from the live VM binding table after the run.
  - **Routed through Legacy reconstruction (deferred to a later slice):**
    struct-, array-, tuple-, and other heap-backed globals. Value-carrying a heap
    array loses its element type and breaks downstream dispatch (observed:
    `det(A)` on a carried Symbolics matrix → "expected numeric array element, got
    Any"). Method / struct / module *definitions* still flow through the shared
    accumulate-and-recompile machinery (S5 removes that).
  - **#9436 captured closures:** a `Closure` global whose environment contains
    factory locals (for example `n` in `add5 = makeadder(5)`) must not be
    re-emitted as a captureless `FunctionRef`. Captured closures now route to
    the `seed_globals` value-carry fallback, so both `Legacy` and `Persistent`
    keep the closure environment intact (`add5(10) == 15`) and the
    migration-seam corpus no longer has a model-divergent closure row.
  - **Two incidental correctness fixes shared by both models:** a definition-only
    eval (bare method extension / `struct` / `macro` def) now returns `Nothing`
    instead of leaking the last injected global's value as a stale result; and the
    `ans` global's compiler type hint (`global_types["ans"]`) is kept in sync at
    the ans-write site (its staleness only surfaced once `ans` was value-carried
    with no correcting init statement).
  - **Runtime ownership is explicit:** the post-run write trace projects only
    unqualified, Main-owned values into the session's `globals` mirror. Import
    publication (including compiler-owned provenance/ambiguity slots) is binding
    metadata and is excluded from value persistence. Qualified writes remain
    module-owned; bindings created only by a called function are carried under
    their qualified runtime names across a fresh VM rebuild, and are discarded
    when that module is redefined (Issue #9784).
  - **C ABI:** unchanged — `check_ffi_abi_version.sh` green, no
    `SUBSET_VM_ABI_VERSION` bump. The `EvalModel` switch is a Rust-only test hook
    (`REPLSession::set_eval_model`), never a C entry point.
  - **Not yet done in S3 (future slices):** flipping the default to `Persistent`,
    broadening the persistent carrier to structs/arrays, and deleting the
    retirement-list symbols. The value→expr path stays live for the routed
    constructs until then.
- **S4 — IMPLEMENTED behind the `EvalModel` selector** (`repl/session.rs`;
  default remains `Legacy`). Scope actually shipped, per the "route the rest
  through Legacy" rule:
  - **Module `__init__` once-initialization (the observable S4 win):** under
    `Persistent`, a USER module's `__init__` runs ONCE — on the eval that
    realizes the module — instead of on every accumulate-and-recompile pass.
    `REPLSession` now tracks `initialized_module_paths` (populated from the
    modules the user typed, captured *before* the package loader appends its
    modules) and, on a later eval that re-merges an already-realized module,
    empties that module's `__init__` body (`suppress_module_reinit`). This
    matches upstream `run_module_init` (a module's `__init__` fires once per
    realization — `julia/base/loading.jl`). Legacy keeps re-firing `__init__`
    each eval; that per-eval re-run is the observably-wrong behavior. The two
    reviewed `models_diverge` families in the migration-seam corpus are exactly
    this: a `__init__` that `push!`es to a module const (`cnt()` stays 1 under
    Persistent vs grows 2, 3, … under Legacy) and a `__init__` that `println`s
    (prints once under Persistent, every eval under Legacy). A `reset()` re-arms
    init (clears `initialized_module_paths`), so a redefinition re-runs
    `__init__` — matching upstream module redefinition and the "reset = fresh
    session" criterion.
  - **Routed through Legacy (unchanged under both models):** the module BODY
    still re-runs every eval, and its mutable const state (`Plots._CURRENT_SERIES`,
    a call-driven `push!`ed module array) is still persisted by
    `restore_module_globals` / `module_globals` (Issue #5296). Package `using X`
    modules are NEVER suppressed: their `__init__` establishes VM-local state
    (backend/registry setup) that MUST re-run in each eval's fresh VM, so they
    stay on the legacy re-run path. Value-carrying module-const state into the VM
    binding table is blocked pre-S5 by three walls — the module body re-runs and
    clobbers any pre-run seed, a module body cannot read an outer/`Main.`-qualified
    carrier in sjulia, and `module_constants` registration is derived from the
    body so the body cannot simply be stripped — so it is deferred to S5's live
    VM. `#5296` therefore stays OPEN; S4 removes the `__init__` re-fire, not the
    body-const restore machinery.
  - **C ABI:** unchanged — `check_ffi_abi_version.sh` green, no
    `SUBSET_VM_ABI_VERSION` bump. The whole change is internal to `REPLSession`
    behind the opaque handle.
  - **Not yet done in S4 (future slices):** true module-body once-run (needs the
    live VM), value-carrying module-const state, and deleting
    `restore_module_globals` / `module_globals` / `extract_module_globals_from_vm`.
- **S5 — PARTIAL / behind the `EvalModel` selector** (`repl/session.rs` +
  `compile/cache.rs`; default remains `Legacy`). An input-delta compile path is
  implemented and proven correct, but — per the honest measurement below — it
  does **not** yet meet the flat-compile exit criterion; the residual is
  structural and needs the live VM + S6. Scope shipped:
  - **Input-delta compile for expression/global evals:** under `Persistent`, an
    eval whose input DEFINES nothing — only top-level expressions and/or global
    (re)assignments — compiles ONLY that input against the accumulated program
    instead of re-`merge_definitions`-ing + recompiling the whole session. The
    session carries a `ReplPersistentCompile` bundle (the accumulated program's
    compiled bytecode + method tables + closure captures + inference snapshot)
    and `repl_delta_compile` reuses it as the **precompiled prefix**, standing in
    for the Base cache in the ordinary reuse path, so only the input is compiled
    and appended. New `CompilerCacheInput.extra_imported_functions` carries prior
    user function names into `imported_functions` so a delta expression can call a
    prior-defined function (otherwise rejected as "not imported").
  - **Key alignment fact:** compilation appends generated functions (inner
    constructors, lifted lambdas) AFTER the source functions, so
    `bundle.compiled.functions` is `[base | generated | user]`, never the
    `[base | user]` shape of the IR. The delta feeds the pipeline the ordinary
    `merge_with_precompiled_base(input)` IR (`[base | NEW user]`, base prefix
    aligned with the bundle) and reuses the WHOLE bundle as the prefix; prior
    user functions are reused verbatim, never reconstructed in IR.
  - **Every DEFINITION routes to the full recompile path** (function/method,
    struct, abstract/primitive type, module, macro, enum, `using`, opaque
    `eval`; and any session that ever realized a module). This is a CORRECTNESS
    requirement, not just conservatism: a forward reference (a function defined
    before its callee) compiles to a baked "Unknown function" trap in the caller;
    Legacy re-compiles the caller once the callee exists, so routing the callee's
    DEFINITION through the full path recompiles the caller and a later delta
    expression reuses the corrected body. Mutual recursion across two evals and
    redefinition are the same case. Verified: the `s5_*` migration-seam rows
    (forward reference, cross-eval mutual recursion, method-calls-earlier,
    redefinition), checked against upstream `julia`, are Legacy ≡ Persistent.
  - **Honest performance finding (the exit criterion is NOT yet met):** the
    benchmark `benches/repl_input_delta_9199.rs` measures per-eval COMPILE time
    (via `REPLSession::last_compile_nanos()`) and shows **Persistent ≈ Legacy —
    both grow with session length, neither is flat.** Reusing prior compiled
    functions removes their re-CODEGEN, but per-eval compile is dominated by
    `build_method_tables` + whole-program bytecode ASSEMBLY (`emit`/finalize/
    method-table seeding over the accumulated program) that both models pay to
    build the fresh VM each eval — the delta does not avoid it. Achieving flat
    compile requires (a) a **live persistent VM** so the accumulated program is
    not re-assembled each eval, and (b) **S6 precise per-name invalidation**
    (#9197 backedges) so a DEFINITION can be delta-compiled without recompiling
    all prior functions to resolve forward references. Both are separate slices;
    S5 as implemented is the correct-but-perf-neutral foundation they build on.
    (A secondary observation: the Base-cache bypass `should_skip_base_cache_for_program`
    fires for programs with many/large user methods, making the Legacy full
    recompile O(all Base) — an independent inefficiency worth its own Issue.)
  - **C ABI:** unchanged — `check_ffi_abi_version.sh` green, no bump. No cache
    format / bincode change (eval-path only, not serialized).
- **S6 — WORLD-AGE CORRECTNESS PINNED behind the `EvalModel` selector; the live-VM
  half deferred** (test/doc-only slice: `tests/repl_differential_9199_tests.rs` +
  `fixtures/repl_differential/migration_seams_9199.toml`; no production code, so
  default remains `Legacy` and the C ABI is untouched). Scope actually shipped:
  - **Reachable world-age is verified correct in BOTH models and pinned by the
    differential corpus.** The upstream invariant — a method redefined in eval N
    is visible from eval N+1's call sites but NOT retroactively applied to eval
    N's already-run code (`julia/src/gf.c`: a call sees only methods defined in
    worlds ≤ its own) — already holds under both Legacy and Persistent because the
    REPL **eval boundary is the world boundary**: the earlier eval's call already
    ran (its global result fixed) before the later eval redefines, and the later
    eval re-resolves against the current method table. New `s6_*` corpus rows +
    two dedicated harness tests
    (`worldage_redefinition_is_not_retroactive_across_evals_9199_s6`,
    `worldage_intra_eval_9400_is_identical_across_models_9199_s6`) pin three
    scenarios, all Legacy ≡ Persistent AND matching upstream `julia`: (a) redefine
    a method between two evals and call it in both (11 → 110); (b) a caller whose
    earlier-defined callee is later redefined re-resolves the callee in the
    current world (21 → 1001); (c) mutual recursion with one method redefined
    across evals propagates the redefinition through the un-redefined partner
    (0/0 → 100/100). Both models realize cross-eval visibility the same way today
    — a **full recompile on any DEFINITION eval** (S5 routes definitions to the
    full path) — so this is correct but not yet a world-counter fact.
  - **The reachability boundary is the script-mode dual #9400 (deferred to the
    live-VM slice).** World-age WITHIN a single eval / single flat program is NOT
    reachable under either model: a same-signature redefinition placed textually
    between two calls in ONE eval resolves BOTH calls to the last definition,
    because sjulia's flat function table **replaces the method in place** — there
    is no world-range-keyed method table, so a call cannot resolve to a method
    that was later overwritten. Confirmed identical for plain top-level redefs and
    for the `@eval` variant (#8452): upstream gives 12, both sjulia models give
    22, IDENTICALLY. The `s6_intra_eval_9400` corpus row + the intra-eval harness
    test pin this equality so the persistent model provably does not make the gap
    worse; the live-VM slice that adds a world-range method table flips these pins
    to the upstream value.
  - **Relationship to #9197 S6 / PR #9485.** The runtime per-name backedge
    invalidation `Vm::note_method_table_mutation_for` (PR #9485) is the machinery
    a future **live persistent VM** will use to make a redefinition visible by a
    world-counter bump + precise cache invalidation instead of a recompile. Under
    today's fresh-VM-per-eval architecture it is exercised **within a single
    eval's** runtime-eval activations (`activate_eval_function`), not across evals
    — there is no VM dispatch cache that survives an eval boundary yet. So the
    "invalidate exactly the dependent cached call sites across evals" target is a
    live-VM prerequisite, tracked with the rest of the reshape.
  - **C ABI:** unchanged — `check_ffi_abi_version.sh` green, no
    `SUBSET_VM_ABI_VERSION` bump (this slice adds no production code, only tests +
    docs). No cache/bincode surface touched.
  - **Not yet done in S6 (the live-VM slice):** a world-range-keyed method table on
    a live persistent VM so (a) a same-signature redefinition keeps the prior
    method resolvable at an earlier world (fixing #9400 / #8452 intra-program), and
    (b) cross-eval redefinition visibility becomes a `current_world` bump +
    `note_method_table_mutation_for` precise invalidation instead of a full
    recompile. Both need the live VM (also the S5 flat-compile-reshape blocker).
- **S7 — LIVE-VM RESHAPE: DESIGN + MEASUREMENT ONLY (this slice).** A feasibility
  assessment of "make `REPLSession` hold ONE live `Vm` across evals" concluded it
  **cannot land soundly in one reviewable PR** (full suite green + differential
  green + C ABI stable); it is decomposed below into sub-slices **LV1–LV6** to be
  landed independently behind the existing `EvalModel` selector, oldest→newest.
  Scope actually shipped in S7:
  - **Where the O(session) cost lives is now measured.** The exit metric tracked so
    far (`last_compile_nanos`) is the compile phase only; a new Rust-internal
    telemetry `REPLSession::last_vm_build_nanos()` times `Vm::new_program`
    separately, and the benchmark `benches/repl_input_delta_9199.rs` now reports
    BOTH. The measurement **localizes the O(session) growth to the COMPILE phase**:
    compile grows with N under both models (measured 3.4x–24x N=0→80, host-noise
    dependent), while VM-build stays small (~single-digit ms) and roughly FLAT
    (~1.0–1.1x N=0→80). `Vm::new_program` *does* re-derive every program-scaled
    table each eval (`call_site_caches = vec![_; code.len()]` `state.rs:282`; the
    predecoded `ExecutableProgram::from_bytecode` `state.rs:280`; `function_slot_maps`
    / `function_name_index` / `type_ancestors` / `struct_hierarchy`), but that cost
    is **dominated by the fixed Base program** — the marginal per-user-definition
    growth is negligible next to it in the practical REPL range. So the live-VM
    reshape's win is flattening the **compile phase** (stop re-assembling the
    accumulated program's method tables + bytecode each eval); vm-build is not the
    bottleneck (a later slice should re-check it at very large N). This is a useful
    negative result: it narrows the reshape's performance target to one phase.
  - **Feasibility evidence (why one PR is unsound):** (1) the compiler has **no
    relocatable-delta output** — `repl_delta_compile` returns a whole
    `CompiledProgram` (`bundle.compiled.clone()`, `compile/cache.rs:2418`) and the
    pipeline re-assembles method tables + emits over the full accumulated program
    (the S5 O(N) residual); a live append needs a NEW compiler contract that emits
    only the delta's functions + main with every internal reference (function
    index, global slot, struct type-id, IP) relocated to the live VM's existing
    index space. (2) `Vm::new_program` (`state.rs:222`) derives ~20 tables from the
    `CompiledProgram`, several IP-indexed over the FLAT `code` vector; each needs a
    proven-correct incremental-growth path. (3) frame-0 global scope
    (`frames: vec![Frame::new_with_slots(program.global_slot_count, None)]`,
    `state.rs:330`) must grow in place for new globals without losing existing ones.
  - **Partial foundations that DO exist (reduce LV risk):** the VM already appends
    to a live method table at runtime for `@eval` —
    `eval_define_function_from_expr` (`vm/builtins_macro/eval.rs:1685`) grows
    `code` (via `Rc::make_mut`), `executable` (`ExecutableProgram::append_bytecode`,
    `executable.rs:49`), `functions`, `function_name_index`, `function_slot_maps`,
    then `activate_eval_function` (`state.rs:1760`) bumps `current_world`/`min_world`
    and does #9197 precise per-name invalidation (`note_method_table_mutation_for`,
    `state.rs:1838`). BUT that path installs a **tree-walked trampoline body**, not a
    compiled body, and appends **no globals/structs/types**, so it is a partial
    precedent for LV3, not a drop-in.
  - **C ABI:** unchanged — telemetry is a Rust-only accessor behind the opaque
    session handle; `check_ffi_abi_version.sh` green, no `SUBSET_VM_ABI_VERSION` bump.
  - **Differential harness / full suite:** unchanged semantics (no eval-path
    behavior change), so both stay green.

---

## Live-VM slice decomposition (Issue #9199 terminal slice)

The S5/S6/S7 findings converge on one conclusion: the epic's exit metric (per-eval
cost independent of session length) is unreachable while `REPLSession` builds a
fresh `Vm` every eval and re-compiles the accumulated program, because the compile
phase re-assembles that whole program's method tables + bytecode each eval. (The
S7 two-phase benchmark shows the O(session) growth is concentrated in the compile
phase; `Vm::new_program` itself, though it re-derives every program-scaled table,
stays small and ~flat in the practical N range because it is Base-dominated.) The
fix — one long-lived `Vm` that each eval mutates with just the input's delta,
compiled against that live VM rather than re-assembled — is a large,
correctness-critical reshape that must NOT land as a single speculative PR. It is decomposed into six
sub-slices, each shippable independently behind the existing `EvalModel` selector
(the default stayed `Legacy` through LV1–LV5 and was flipped to `Persistent` at
LV6), each gated on the differential harness + full suite +
`check_ffi_abi_version.sh`, oldest→newest dependency order. "Persistent-live" is
the target model these build toward; a construct routes to it only once its
differential rows are green, everything else keeps flowing through today's
`Persistent`/`Legacy` paths.

| Sub-slice | Scope | Retires (when green) | Depends on |
|---|---|---|---|
| **LV1** | **Expression-only delta on a live VM.** `REPLSession` holds `Option<Vm>`; a full-compile eval stores it instead of dropping. For a delta input that DEFINES nothing and introduces NO new global binding (only expressions + reassignment to existing globals — the S5 `delta_eligible` case minus new bindings), compile emits ONLY the new-main bytecode relocated to the live VM's index space, the VM appends it (`code`+`executable`+`call_site_caches` grow) and re-enters `run()` at the new entry, **preserving frame-0 globals / struct heap / dispatch caches / world**. `reset()` drops the live VM. | nothing yet (Legacy/Persistent stay); proves append + re-enter + the relocatable-delta compiler contract. | S5, S6 |
| **LV2 — LANDED** | **Relocatable-delta compiler contract + live-append wiring (the crux).** Emits ONLY the new user main with global slots SEEDED from the live frame-0 (existing globals keep their slot, a new global grows frame-0 in place, `Vm::grow_global_slots`), skips the O(session) prefix assembly, and re-enters the held `Vm` via the landed `reenter_appended_main`. Covers expression / global-(re)assignment deltas (no lambda-lift, no hard-scope `let`, no user module). COMPILE flat vs N for that subset (benchmark). See "LV2 — LANDED" below. | `inject_globals` / `seed_persisted_globals` / `Vm::new_program` for the covered deltas. | LV1 |
| **LV3 — LANDED (Main-source method family)** | **Main-owned FUNCTION definitions and mutations on the live VM (compiled, not tree-walked).** Compile each source method plus its marker-specific transitive caller refresh slice to real bytecode relocated to the live index space; append every parallel function/specialization table; and publish the primary+refresh group with one source-ordered world increment. Brand-new methods, extensions, and same-signature replacements are live for ordinary, `where`, keyword, vararg, and combined `where`+keyword declarations. Syntax is not the gate: ownership admits the source method and relocatable extraction independently proves the complete aligned body/specialization/activation surface before mutation. Base/preload-owned extensions and structurally unextractable generated helpers remain on the full fallback. See "### LV3 — LANDED" below. | `merge_definitions` and accumulated-program recompilation for the covered Main-source method definitions. Redefinition visibility and caller invalidation are now method-world transactions instead of rebuild side effects. | LV2 |
| **LV4 — LANDED (new-concrete-struct subset)** | **New concrete STRUCTS on the live VM (compiled type-registry append).** A brand-new non-parametric, no-inner-constructor struct is compiled against the reused prefix (its `type_id` = its aligned index in `struct_defs`) and reserved in a private registry tail. Each source-ordered `DefineEvalStruct` marker activates that type in the live registries (`struct_defs`, `struct_def_name_index`, `struct_hierarchy`, the precomputed `type_ancestors` closure) + performs a coarse dispatch-cache retire — instead of a whole-program `Vm::new_program`. A catchable error commits only the reached definition prefix. LANDED for BRAND-NEW CONCRETE structs; abstract / primitive / enum types, parametric / inner-constructor structs, and struct redefinition fall back to the full recompile (LV4b). See "### LV4 — LANDED" below. | `merge_definitions` (concrete structs) for the covered subset; the accumulated-program recompile for those definition evals. | LV3 |
| **LV5 — LANDED (simple-user-module subset)** | **Modules realized once on the live VM.** A SIMPLE user module (functions + const/global bindings + optional `__init__`) is realized on its DEFINITION eval (full recompile, which parks that VM); every later eval that only REFERENCES the module re-enters the parked VM, so its mutable const state persists IN the VM — retiring the `restore_module_globals` fakery **on the live path** for that subset (#5296, the Plots symptom, was already closed 2026-05-30; LV5 does not reopen it). LANDED for simple user modules; LV5b (#9723) widened the live path to submodules and module-level struct/abstract/primitive types, and fixed module-redefinition state reset (upstream semantics, #10232); package `using X` modules, inner `using`, module-level macros/type-aliases, and `baremodule` stay fail-closed on the full recompile (LV5b remainder). See "### LV5 — LANDED" below. | `restore_module_globals` on the live path for the covered subset (not a `Closes #5296`; that was already closed). | LV4 |
| **LV6 — FLIPPED (default = Persistent)** | **Default flip.** The two audit blockers (#9786, #9787) are fixed and merged, the differential harness is 8/8 green, and both a full `cargo nextest run --release` (default features) AND a full `--features repl` run are green under the flipped default — so the production REPL default is now `EvalModel::Persistent`. The epic's production goal (a persistent-VM REPL as the shipped default) is **MET**. The `EvalModel` switch + the Legacy variant are kept INTACT: the differential harness needs Legacy as its oracle. See "### LV6 — FLIPPED" below. | Nothing yet — retirement (Legacy path + retirement-list symbols + the `EvalModel` switch) is **LV6b (Issue #9784)**, gated behind this flip. | LV1–LV5 |

**Cross-cutting prerequisite for LV1 (the crux):** a **relocatable-delta compiler
output**. Today `repl_full_compile`/`repl_delta_compile` return a self-contained
`CompiledProgram` (base|generated|prior-user|new) whose method tables + bytecode
are emitted over the WHOLE program (`compile/cache.rs:2345-2430`). LV1 needs the
pipeline to instead emit only `{new functions, new main bytecode}` with every
internal reference (function index, global slot, struct type-id, absolute IP)
expressed relative to — or relocated onto — the live VM's existing index space, so
the VM can splice it without renumbering. This is a new compiler contract, not a
tweak, and is the single largest piece; it is why LV1 is scoped to the narrowest
(expression-only, no new bindings) case that still exercises the full append +
re-enter path.

**Runtime prerequisites already partly built (see the S7 status entry):**
`ExecutableProgram::append_bytecode`, the `@eval` live-method-append path, and the
#9197 `activate_eval_function` / `note_method_table_mutation_for` world-age
machinery. LV3 upgrades the `@eval` append from tree-walked to compiled; LV1/LV2
add the missing frame-0 global-slot growth and the `call_site_caches` / IP-indexed
table growth that `@eval` sidesteps by only appending a fixed 2-instruction
trampoline.

**Recommendation to the lead:** land LV1–LV6 as separate PRs, each with its own
differential rows and a benchmark reading (LV1 should flip the covered subset's
COMPILE phase to flat — the phase the S7 benchmark localized the O(session) cost
to — provable on that same two-phase benchmark). Do NOT attempt
LV1–LV6 in one PR — the relocatable-delta compiler + the ~20 incremental VM tables
+ frame-0 growth are each independent correctness surfaces that need their own
review and full-suite gate (#5966: the eval-model reshape reaches every fixture).

### LV1 — PARTIAL: runtime append/re-enter primitive landed; the compiler contract is the blocker

The LV1 slice split cleanly into two halves, and the split IS the finding:

- **The RUNTIME half landed and is proven** (`Vm::reenter_appended_main` +
  `Vm::code_len`, `subset_julia_vm_vm/src/vm/state.rs`): splice a slot-free `main`
  onto a live `Vm` (grow `code` copy-on-write, `executable.append_bytecode`, the
  IP-indexed `call_site_caches`, `source_map`), reset ONLY the transient per-run
  state (stack, frames>0, output, error/exception, per-run test counters, RNG),
  and re-enter `run()` — while LEAVING frame-0 module globals, the struct heap,
  every dispatch / specialization cache, the interned type ids, and
  `current_world` / `dispatch_generation` intact. Exercised by
  `repl::session::lv1_live_vm_tests` (append a `main` that reads a live global by
  name → correct result + frame-0 preserved; repeatable; and the `Option<Vm>`
  foundation: a `Persistent` eval parks its VM, `reset()` drops it, `Legacy`
  never holds one). `REPLSession` now holds `Option<Vm>` (parked after each
  successful `Persistent` eval, with the session-managed `ans` mirrored into
  frame-0 so a future live read is current; dropped on error and by `reset()`).
  C ABI untouched (`check_ffi_abi_version.sh` green, no bump).

- **The COMPILER half is the blocker and is NOT wired.** The runtime primitive
  needs an *isolated, prefix-aligned, slot-free* delta `main` to splice, and
  `repl_delta_compile` does not produce one. Measured on the LV1 corpus, its
  output for an expression delta has (1) `compiled.entry == live_code_len + ~405`
  — it re-emits the **base-main prefix** ahead of the user main, so the `main`
  assumes ~405 instructions the live VM does not have and cannot be appended at
  `live_code_len`; and (2) a ~1673-instruction `main` full of `StoreSlot*` — the
  **accumulated global re-inits** — because `build_slot_info` assigns frame slots
  from *that delta's own tiny main*, unaligned with the live VM's frame-0 (a
  global STORE lands on the wrong slot, e.g. the `InteractiveUtils` module
  binding; only read-only globals stay safe, since they compile to *name-based*
  access that resolves against frame-0 regardless of slot layout). Isolating the
  user delta + relocating its global slots onto the live index space is exactly
  the "relocatable-delta compiler output" crux above — the single largest piece —
  and it cannot land soundly in the same PR as the runtime primitive. LV1's
  headline metric (flat COMPILE for the covered subset) is therefore NOT yet met;
  the benchmark is unchanged. **LV2 (or an LV1b) picks up the compiler contract**:
  teach the delta compile to emit only `{new main}` with global slots seeded from
  the live layout (no base-prefix re-emit, no accumulated-init re-emit), so the
  landed `reenter_appended_main` primitive has an alignable `main` to splice.

### LV2 — LANDED: relocatable-delta compiler contract + live-append wiring (the crux)

LV2 built the relocatable-delta compiler output the LV1 note flagged as the
blocker and WIRED it to the LV1 runtime primitive, so the covered subset now runs
on the held live `Vm`. **The headline metric is MET for that subset.**

- **The relocatable-delta compiler contract** (`repl_relocatable_delta_compile`,
  `compile/cache.rs`). Driven by a new `CompilerCacheInput.global_slot_seed`
  (the live VM's `global_slot_names`), the pipeline now:
  - **seeds the main block's global slots from the live frame-0 layout**
    (`build_global_slot_info_seeded`, `slot.rs`): every existing global keeps its
    live slot index, a brand-new global appends — so a delta's `StoreSlot`/`LoadSlot`
    aligns with frame-0 instead of colliding (the exact "global STORE lands on slot
    0 → `InteractiveUtils` module" bug the LV1 note described);
  - **skips assembling the O(session) Base/prior code prefix** (`!assemble_prefix`
    in `finalize`, `pipeline_ctx.rs`) — `code` is left as the freshly-compiled suffix
    (fresh functions + base main + user main); and
  - **installs a peephole fusion barrier at the base-main / user-main seam**
    (`optimize_with_boundaries`, `peephole.rs`) and records `user_main_entry`, so
    `code[user_main_entry..]` is a self-contained, extractable user main. The
    function then slices it out and relocates its intra-block jumps onto the live
    VM's code tail (function calls are index-based, slots already live-aligned).
- **The wiring** (`REPLSession::try_live_delta_run`, `session.rs`): under
  `Persistent`, an LV1/LV2-eligible delta compiles the relocatable main, grows
  frame-0 for any brand-new global in place (`Vm::grow_global_slots`), and
  re-enters the held VM via the landed `reenter_appended_main` — **no
  `Vm::new_program`, no re-inject, no re-run**. The prior full compile's bundle is
  the fixed prefix; a delta never replaces it (a delta adds no reusable functions,
  so replacing it would grow the prefix unboundedly).
- **The metric (MET for the covered subset).** `benches/repl_input_delta_9199.rs`
  now measures `Persistent` COMPILE ~**FLAT** vs `N` (measured N=0→80: ~1.1x vs
  Legacy ~22x) and `Persistent` vm-build **0** (live re-enter, no fresh build). The
  covered subset is expression / global-(re)assignment deltas that: define no
  Julia-visible generic, contain no unsupported hard-scope `let`/`@testset`,
  reference no unsupported user module, and call only already-live functions or
  structurally extracted current-input HOF/do/generator helpers.
- **The S5 dead-path fix (LV1 note S5).** Delta eligibility is gated on
  `module_globals.is_empty()` (no captured #5296 state) instead of
  `!self.modules.is_empty()`; the default `InteractiveUtils` auto-import (tracked in
  `auto_import_modules`) no longer makes every default-session delta inert.
- **Correctness backstops** (each `Ok(None)` / gate → fall back to full recompile,
  Legacy ≡ Persistent verified by the differential harness):
  - a generated callable whose complete transitive helper body, specialization,
    or static generator target cannot be proven inside the aligned append region
    (`user_main_calls_only_existing_functions` remains conservative); ordinary
    lambda, do-block, generator-body, and generator-predicate helpers are extracted
    and installed on the held VM;
  - a hard-scope `let`/`@testset` shadowing a live global (its `ForgetLetLocals`
    would CLEAR the live global) — routed to the full path by an IR-level gate
    (`program_main_has_hard_scope_block`), AND such an eval's VM is NOT parked
    (its frame-0 is left inconsistent with `self.globals`);
  - a user module referenced by the delta ("Unknown module") or a preload splice.
- **C ABI:** unchanged — `check_ffi_abi_version.sh` green, no `SUBSET_VM_ABI_VERSION`
  bump; no cache/bincode format change (the relocatable output is a runtime slice,
  not serialized).
- **Not yet done (LV2b remainder):** an opaque or future callable-lowering shape
  whose complete target/alignment surface cannot be structurally decoded keeps
  the fail-closed full-recompile path.

### LV3 — LANDED: compiled Main-source method definitions on the live VM

LV3 upgrades the `@eval` tree-walked append (`eval_define_function_from_expr`) to
a COMPILED append and wires it to the eval path. Under `Persistent`, a delta that
defines or mutates Main-owned source methods compiles those bodies and the exact
transitive caller-refresh slice, then splices them onto the held live `Vm` — no
accumulated-program recompile. The path covers brand-new methods, new-signature
extensions, and same-signature replacements for ordinary, parametric (`where`),
keyword, vararg, and combined `where`+keyword declarations. **The exit criterion
is now MET for definition deltas of the covered method family** as well as the
LV2 expression subset.

- **The layout fact that makes it clean** (verified by probing a real base-cached
  session): the relocatable-delta compile lays the NEW user *source* functions out
  CONTIGUOUSLY at the front of the fresh region `[P..P+u]` (`P` =
  `prev.bundle.compiled.functions.len()`, `u` = the input's user-function count),
  followed by the ~34 deterministic re-lifted trailing Base closures (`#sel` /
  `#fused` / `#…_curried` / `#__lambda_nested_*`). So the new functions extract by
  name-matching the front of `[P..]` against `input.functions`, and — because the
  caller guarantees the live VM still holds EXACTLY `P` functions — install at
  aligned live indices `[P..P+u]` with NO function-index relocation (a body's calls
  to a base/prior function stay `< P`; calls to a same-eval sibling land on that
  sibling's aligned index in `[P, P+u)`). Marker-less callable helpers referenced
  by the user main or an installed body extend that aligned region transitively;
  an unresolved reference into the unrelated trailing duplicate region rejects
  and falls back. Lowering helpers may occur before, between, or after source
  methods; visibility and recovery derive source identity from activation
  membership rather than function-table position.
- **Compiler contract** (`repl_relocatable_delta_compile`, `compile/cache.rs`):
  `AppendableDelta` now carries `new_functions: Vec<AppendableFunction>` — each with
  its bytecode (jumps relocated onto the live tail) and a `FunctionInfo` whose
  `entry`/`code_start`/`code_end` are the final live positions, spliced BEFORE the
  user main (which is based after them). `source_function_indices` records the
  final aligned primary indices in activation order. The broadened appendability
  gate discovers closure and static-generator targets transitively, then runs
  `user_main_calls_only_existing_functions` at the resulting installed threshold
  for BOTH the user main and every installed body.
- **Runtime primitive** (`Vm::install_appended_function`, `vm/state.rs`): appends
  the pre-relocated body, grows `executable` / `call_site_caches` / `source_map`,
  registers into every per-function table (`functions`, `function_name_index` bare
  + qualified short name, `function_slot_maps`, `native_array_exempt_functions`),
  then calls `activate_eval_function` (world bump + `min_world` + #9197 precise
  invalidation) — the exact activation `@eval` uses, now over a compiled body.
- **Wiring + soundness gates** (`REPLSession`, `session.rs`):
  `input_defines_only_new_generic_functions` admits Main-owned source methods,
  including `where` and keyword syntax, plus lowering-generated anonymous
  callable helpers; it still rejects Base/preload-owned generics. Helpers never
  enter the Julia-visible `function_names` / method-source snapshot.
  `ReplMethodIdentity` and
  `ReplMethodSourceSnapshot` compute marker-specific mutations and transitive
  caller refreshes; `repl_relocatable_delta_compile` then proves function-index,
  specialization-row, closure-target, global-slot, and activation-group alignment
  independently before taking the VM. `try_live_delta_run` requires
  `vm.functions_len() == prev.prefix_function_count()`
  before installing (runtime `@eval` can grow the live count past `P`, misaligning
  the indices → fall back). Since Issue #9784, a successful definition append also
  advances the reusable compiler snapshot to the exact relocated live layout.
  Expression-only mains omitted from the snapshot are represented by inert `Nop`
  gaps so later function entries retain their live offsets. The snapshot commits
  only after the appended main succeeds; consecutive function/type definitions and
  references therefore remain on the live path without a stale-prefix refresh.
  A new binding named by a reused prefix `ThrowUndefVarError` still falls back to
  the full path: that rebuild is required to repair cross-eval forward references
  and mutual recursion. Persisted appended methods are marked prefix-visible only
  after success, matching the live install's world activation on a later VM rebuild.
- **World-age and source chronology.** Every covered definition or mutation is a
  `current_world` bump on the live VM, not a recompile. A source marker publishes
  its primary method and caller-refresh bodies atomically. Calls before a later
  replacement select the reached method; calls after the marker see the new method.
  Direct, dynamic, specialized, keyword, and reflection paths all consult the same
  method-world state. On a catchable error, only reached activation groups enter the
  reusable compiler snapshot; dormant suffix methods remain absent from later
  dispatch and reflection.
- **The metric (MET for the covered subset).** `benches/repl_input_delta_9199.rs`
  measures curve `[B] DEFINITION deltas`: a brand-new generic definition at
  session length N compiles ~FLAT under `Persistent` (live-append, vm-build 0) vs
  the historical Legacy O(N). Differential regressions additionally assert
  `last_vm_build_nanos() == Some(0)` for ordinary, `where`, keyword, vararg, and
  combined method definitions, extensions, replacements, caller refreshes, and
  reached-prefix recovery.
- **C ABI:** unchanged — `check_ffi_abi_version.sh` green, no `SUBSET_VM_ABI_VERSION`
  bump; no cache/bincode format change (the compiled bodies are a runtime slice, not
  serialized; `Instr` variant count unchanged).
- **Not yet done (LV3b remainder):** Base/preload-owned generic extensions and
  any future lowering shape whose complete body/specialization/activation surface
  fails structural extraction. Module-owned definition activation remains part of
  LV5b; opaque runtime `eval` remains a separate #9784 transaction slice.

### LV4 — LANDED: compiled new-concrete-struct definitions on the live VM

LV4 extends the compiled live-append from FUNCTIONS (LV3) to concrete TYPES: under
`Persistent` a delta that DEFINES only brand-new, non-parametric,
no-inner-constructor structs compiles just that delta and appends its type
registries to the held live `Vm` — no accumulated-program recompile. **The exit
criterion is now MET for struct-definition deltas of the covered subset** (curve
`[C]`), on top of the LV2 expression subset `[A]` and the LV3 function subset `[B]`.

- **The layout fact that makes it clean.** A concrete struct's `type_id` IS its
  index in `struct_defs`, baked into every `NewStruct(type_id, ..)`, and — for a
  non-parametric struct with only the default constructor — the constructor call
  is INLINED as `NewStruct`, not a generated function. So a bare struct definition
  adds a `struct_def` but NO `compiled.functions` entry. The relocatable-delta
  compile, seeded from the reused prefix's `struct_defs` (length `S`), lays the
  input's new concrete structs out CONTIGUOUSLY at the aligned tail `[S..S+u]`; the
  caller guarantees the live VM holds exactly `S` struct defs, so each installs at
  exactly the `type_id` the delta baked — NO type-id relocation.
- **Compiler contract** (`repl_relocatable_delta_compile` / `AppendableDelta`,
  `compile/cache.rs`): `AppendableDelta` now carries `new_struct_defs:
  Vec<StructDefInfo>`, extracted by the pure, unit-tested
  `extract_appended_struct_defs`. Its registry-level fail-closed contract: install
  the aligned tail `[S..S+u]` ONLY IF (1) `compiled.struct_defs.len() == S + u`
  (the compile appended no EXTRA struct — a lazily-instantiated parametric type
  would push the count past this and its `type_id` would be referenced but
  uninstalled) AND (2) each tail entry's NAME matches the input's declared struct
  in order. Any violation ⇒ full recompile. Because every emittable `NewStruct(tid)`
  has `tid < compiled.struct_defs.len()`, this count+name check at the REGISTRY
  level is a complete soundness gate — no per-instruction struct-id scan is needed.
  `ReplPersistentCompile` gains `type_names` + `prefix_struct_def_count()` +
  `defines_type()` so a redefinition of an existing type routes to the full path.
- **Runtime primitive** (`Vm::reserve_appended_types` + `DefineEvalStruct`,
  `vm/state.rs` / `vm/dispatch/execute.rs`): compilation reserves each new
  `StructDefInfo` in a private tail without installing its Julia binding. When the
  appended main reaches the corresponding source marker, activation grows EVERY
  per-type table `Vm::new_program` derives from `struct_defs` —
  `struct_def_name_index`, `struct_hierarchy` (`insert_if_absent`, which also
  records the type NAME in the thread-local type-name registry, #9464), and the
  precomputed transitive `type_ancestors` closure (extended incrementally for the
  new leaves via `append_type_ancestors`, which existing entries never need — a
  brand-new leaf cannot be an ancestor of a prior type). Then a coarse
  `note_method_table_mutation` retire, because a new struct can turn a
  previously-failed specialization / dispatch into a success and its subtype edge
  may participate in an existing `f(::Abstract)` dispatch. Runtime `eval` uses the
  same reservation primitive and activates at its own struct-definition marker, so
  the newly defined type remains immediately constructible.
- **Wiring + soundness gates** (`REPLSession`, `session.rs`):
  `input_defines_only_new_types` admits ONLY brand-new non-parametric,
  no-inner-constructor structs whose name is absent from the prefix's types AND
  functions AND prior user structs (a redefinition / name collision full-recompiles).
  `try_live_delta_run` requires `vm.struct_defs_len() == prev.prefix_struct_def_count()`
  before reservation (the struct analogue of the LV3 function-count guard).
  `PushDataType`, `NewStruct`, `NewStructSplat`, and the typed fused-return path
  fence access to unreached reserved IDs with a catchable `UndefVarError`; called
  function bodies therefore cannot observe a later top-level struct definition.
  Function and type markers append to one `ReplDefinitionActivation` trace in
  exact source order. Success commits the complete trace; a catchable error commits
  only the reached prefix to the VM, compiler snapshot, and session mirrors and
  discards the reserved suffix. A later `p = Pt(3)` and consecutive new struct
  definitions therefore compile against the live-aligned committed `struct_defs`
  tail without a full refresh.
  The fresh-VM S5 delta path is also gated on `program.structs.is_empty()` so a
  struct definition that misses the live path takes the FULL recompile (which
  refreshes the prefix), never the prefix-preserving fresh delta.
- **World-age / registry soundness.** A new struct is a leaf TYPE, not a method: it
  carries no `min_world` and cannot retroactively affect already-run code (its name
  did not exist earlier), so no `current_world` bump is needed — only the coarse
  dispatch/specialization-cache retire (sound, and cheap since type defs are rare).
  Cross-eval struct visibility and struct redefinition (Julia ≥ 1.12 allows a wider
  layout) are pinned Legacy≡Persistent AND against upstream `julia` 1.12 by the
  `lv4_*` migration-seam rows.
- **The metric (MET for the covered subset).** `benches/repl_input_delta_9199.rs`
  now prints a third curve `[C] STRUCT DEFINITION deltas`: a brand-new
  non-parametric struct at session length N compiles ~FLAT under `Persistent`
  (live type-append, vm-build 0) vs `Legacy` O(N).
- **C ABI / cache:** C ABI unchanged — `check_ffi_abi_version.sh` green, no
  `SUBSET_VM_ABI_VERSION` bump. The serialized instruction enum gained
  `DefineEvalStruct`, so the Base cache format version is 165 and its schema
  fingerprint includes the new variant; `.sjvmbc` compatibility is unchanged.
- **Not yet done (LV4b — DEFERRED):**
  - **Abstract / primitive / enum type LIVE-REGISTRY appends.** The cross-eval
    *binding* persistence these needed **LANDED with #9701**: `store_definitions` /
    `merge_definitions` now accumulate + re-fold `program.abstract_types` /
    `primitive_types` / `enums` symmetrically with structs (same-name redefinition
    replaces in place; `reset()` clears), so a prior-eval abstract type stays a
    resolvable `Type` value — `x isa PriorAbstract`, a struct subtyping it, and
    `f(a::PriorAbstract)` dispatch all work in BOTH eval models (pinned by the
    `t9701_*` migration-seam rows and the `*_9701` unit tests, verified against
    upstream `julia` 1.12). What remains LV4b: an input DEFINING an
    abstract/primitive/enum type still routes to the full recompile
    (`persistent_delta_eligible` rejects it) — the compiled live type-append
    (`Vm::reserve_appended_types` + `DefineEvalStruct`) covers only concrete structs, so these
    definition evals stay O(session) until the registry appends land.
  - **Parametric (`where`) structs** (not registered in `struct_defs` — instantiate
    lazily, so no positional `type_id` to align) and **inner-constructor structs**
    (generate helper functions the front-of-fresh-region extraction cannot isolate).

### LV5 — LANDED: simple user modules realized once on the live VM (retires the live-path fakery for the subset)

LV5 makes a SIMPLE user module a live object that is realized ONCE and thereafter
referenced on the held live `Vm`, instead of re-running its body — and faking its
mutable state via `restore_module_globals` — on every eval. **The #5296 fakery is
retired for the covered subset on the live path.**

- **The mechanism (no new relocatable-module compiler contract needed).** A
  `module M … end` DEFINITION still takes the full recompile (its body runs once,
  `__init__` fires once via the S4 `suppress_module_reinit`, the module is realized
  on the fresh VM) — but under `Persistent` that fresh VM is PARKED (LV1). Its
  frame-0 already holds the module's const globals (qualified `M.const`) and its
  method tables hold the module functions (qualified `M.f`). A LATER eval that only
  REFERENCES the module (`M.f()`, `M.const`) re-enters that parked VM via the LV2
  relocatable-delta path — so the module's mutable const state (`push!`ed into a
  `const` array by a called function) persists IN the VM across evals with NO
  `restore_module_globals` reconstruction. Benchmark-wise these reference evals flip
  to vm-build 0 (live re-enter) where they previously full-recompiled every eval.
- **The one blocker LV5 solves: module reference RESOLUTION in a delta.** A delta's
  own IR carries no `modules` (it only references them), so the compiler's
  `module_functions` / `module_constants` maps were empty and `M.f()` compiled to
  "Unknown module". LV5 carries the prior modules' SURFACE — a
  `cache::ReplModuleMetadata` (function / constant / export NAME sets, built once
  per full recompile by `collect_module_info`, stored in `ReplPersistentCompile`) —
  into `repl_relocatable_delta_compile` via the new
  `CompilerCacheInput::extra_module_metadata`. `collect_module_metadata` folds it
  into `module_functions` / `module_exports` / `module_constants` (metadata ONLY —
  no module body is re-emitted; the bodies already live verbatim in the reused
  prefix). The function INDEX still comes from the authoritative prefix method
  tables, so a missing/incomplete surface entry only downgrades to the
  full-recompile fallback (`Ok(None)` / compile error ⇒ fall back), never a
  miscompile.
- **Eligibility gate — FAIL-CLOSED at the session level**
  (`REPLSession::session_modules_persistable`). A module-bearing session is admitted
  to the (live-only) delta path ONLY when it loaded NO package (`using X` beyond the
  stateless `InteractiveUtils` auto-import) AND every non-auto module is a SIMPLE
  persistable user module (`module_is_simple_persistable`: no inner
  `using`/`import`, no module-level macros or type aliases, not a `baremodule`,
  every submodule recursively simple-persistable — submodules and module-level
  struct/abstract/primitive types are ADMITTED since LV5b, Issue #9723; AND —
  `module_bindings_fully_mirrorable`, checked per submodule — every
  binding the RESOLUTION collector `collect_module_body_binding_names` sees is also
  tracked by the STATE-MIRROR collector `collect_assign_vars_in_stmts`, i.e. `mirror
  ⊇ resolution`). That last check closes a two-collector asymmetry (codex review
  r3539823948): the resolution collector recurses into `if`/`begin` branches,
  `AssignExpr`, and empty `LetBlock` (Issue #7917). Since Issue #9729 the
  state-mirror collector also tracks module-top-level `if` branches, so
  `if`-wrapped `const` bindings are live-path eligible and survive later
  full-recompile fallbacks. Bindings the mirror still cannot track (for example
  `AssignExpr` or empty `LetBlock`) route to the full recompile path (LV5b).
  The per-shape classification (which statement shapes each collector sees, and
  the resulting live/fallback classification) is pinned by the coverage table
  `repl::session::lv5_mirror_coverage_tests_9996` (Issue #9996): a collector
  coverage change without an explicit expectation update fails that table, and
  its difference-set test names exactly the constructs where resolution sees a
  binding the mirror does not (currently `AssignExpr` and empty `LetBlock`).
  Package modules (whose `__init__` sets up VM-local state that MUST re-run each eval) and
  structurally richer modules likewise keep the full recompile path (LV5b). An input
  that DEFINES a module always full-recompiles (it realizes + parks the VM). A
  module-bearing session has NO fresh-VM delta path
  (a fresh `Vm` never re-runs the module body, so its realized state would be
  missing) — `has_stateful_user_module` routes such a session's non-live deltas to
  the full recompile.
- **State coherence across live↔full transitions.** `module_globals` is refreshed
  after EVERY eval — including live deltas, now over the session's own modules
  (`self.modules`), not just `program.modules` — so if a module-bearing session
  later drops onto the full recompile path (e.g. a hard-scope `let`, which is not
  parked), `restore_module_globals` re-materializes the CURRENT module state, not a
  stale snapshot. Verified by `module_state_coherent_across_live_and_full_fallback_9199`.
- **World-age / once-init.** LV5 does NOT change realization or `__init__` timing:
  the module is still realized (and `__init__` still fires) exactly once on the
  DEFINITION eval, exactly as S4 established; LV5 only stops DISCARDING that realized
  VM on subsequent reference evals. `reset()` drops the live VM + module metadata, so
  a redefinition re-realizes and re-fires `__init__` (reset = fresh session).
- **The metric.** The `s4_*` and new `lv5_*` migration-seam rows (mutable module
  const across evals, `__init__`-once, cross-eval module function call) are Legacy ≡
  Persistent AND match upstream `julia`; the reference evals now run on the live VM
  (unit test `module_reference_delta_appends_to_live_vm_9199`, vm-build 0).
- **C ABI:** unchanged — `check_ffi_abi_version.sh` green, no `SUBSET_VM_ABI_VERSION`
  bump; no cache/bincode format change (`ReplModuleMetadata` is an eval-path runtime
  structure, not serialized; `Instr` variant count unchanged).
- **Relationship to #5296.** #5296 (the `Plots._CURRENT_SERIES` symptom) was
  already CLOSED as completed (2026-05-30) — the `restore_module_globals` fakery
  fixed the observable Plots bug long before LV5. LV5 does not reopen it; it
  *additionally* retires that fakery **on the live path** for the simple-user-module
  subset (their mutable const state now persists directly in the live VM, no
  reconstruction). `restore_module_globals` / `module_globals` /
  `extract_module_globals_from_vm` still exist and are still USED — for the full
  recompile path (Legacy, module DEFINITION evals, every LV5b module) and to keep
  `module_globals` coherent for the live↔full fallback — so this is not a `Closes
  #5296`; LV6 deletes the machinery outright once LV5b covers the remaining module
  kinds.
- **LV5b (Issue #9723) — per-module-kind widening, PARTIALLY LANDED:**
  - **Submodules — LIVE.** A module whose submodules are recursively
    simple-persistable is admitted: the carried surface is keyed by qualified
    path (`M.Sub`, `collect_module_info` recurses), the state mirror
    (`collect_module_constant_paths` → `extract_module_globals_from_vm` /
    `restore_module_constants`) recurses the SAME qualified paths, and
    `module_bindings_fully_mirrorable` is enforced per submodule — so
    `M.Sub.f()` / `M.Sub.const` resolve on the live path and nested mutable
    const state survives a live→full-recompile fallback
    (`lv5b_module_kinds_tests_9723`, `lv5b_submodule_*` differential rows).
  - **Typed modules (module-level struct / abstract / primitive types) —
    LIVE.** The reused compile prefix already holds the qualified type defs
    (`M.Pt` in `struct_defs` etc., registered at full-recompile time) and the
    carried surface resolves `M.SomeType` (type names are in the
    `module_functions` name set), so `M.Pt(3)` / `isa`-dispatch references
    compile in a delta. An unresolvable reference degrades to the
    full-recompile fallback (fail-safe), never a miscompile
    (`lv5b_typed_*` rows).
  - **Module REDEFINITION — semantics fixed; still full-recompiles.** A
    redefinition eval always takes the full recompile (`program.modules`
    non-empty) and re-realizes + re-parks the VM; making it a live update
    shares the LV3b/LV4b redefinition/world-range story and stays deferred.
    What LV5b fixed (Issue #10232, BOTH models diverged from upstream):
    `restore_module_globals` no longer resurrects the OLD module's persisted
    state into a module the current input (re)defines — upstream `julia`
    REPLACES the module binding, so the new module starts from its own
    initializers (`lv5b_redef_*` rows pin bump→1,2, redefine, bump→1). The
    skip filter is read-only, so an errored redefinition eval leaves the old
    module's state intact.
  - **Package `using X` modules — stay FAIL-CLOSED (deliberate).** A package
    module's `__init__` establishes VM-local state (backend/registry setup)
    that MUST re-run in each eval's fresh VM under the legacy re-run path; the
    bundled Plots/Symbolics/Distributions packages rely on this. Realizing such
    a module once and re-entering the parked VM would skip that
    re-establishment, so `session_modules_persistable` keeps rejecting any
    session with a non-auto `using X`. Persisting (or safely re-establishing)
    that VM-local `__init__` state on the live VM needs its own design — a
    dedicated follow-up, not a gate relaxation.
  - **Still fail-closed (LV5b remainder):** inner `using`/`import` (re-exported
    surface unresolvable + package-init semantics), module-level macros (a
    delta's lowering needs the macro BODY, which the name-set surface does not
    carry), type aliases (no alias-target mapping carried), and `baremodule`.
  - **Modules with a binding the state-mirror can't track** (`AssignExpr`, empty
    `LetBlock` — rejected by `module_bindings_fully_mirrorable`). These need the
    live VM to BE the source of truth (no `module_globals` mirror), so they land
    with the LV6 cutover. `if`-wrapped `const` bindings are no longer in this
    class after Issue #9729. The exact membership of this class is asserted by
    `resolution_minus_mirror_difference_set_is_exact_9996` — moving a shape out
    of it (or into it) requires updating that expectation, the coverage table,
    and this section together (Issue #9996).

**Testing guidance — invariant assertions vs. path-policy assertions (Issue
#9996; prevention for #9989 / PR #9995).** LV5/LV5b tests must assert the
DURABLE CONTRACT first, and the live-vs-full PATH POLICY only when collector
coverage explicitly requires that policy:

- **Invariant assertions (always write these):** Legacy ≡ Persistent model
  equality on the observed value sequence; module state preserved across a live
  path followed by a full-recompile fallback; and `mirror ⊇ resolution` for any
  module admitted to the live path. These hold regardless of which path an eval
  takes, so they survive intentional collector-coverage changes.
- **Path-policy assertions (write sparingly, and anchor them):** `vm-build == 0`
  (took the live path) or `vm-build != 0` (stayed on the full recompile) encode
  the CURRENT coverage of `collect_module_body_binding_names` /
  `collect_assign_vars_in_stmts` / `module_bindings_fully_mirrorable`. Only
  assert a path policy when the mirrorability classification requires it, and
  keep the expectation tied to the coverage table
  (`lv5_mirror_coverage_tests_9996`) so an intentional coverage change updates
  the policy assertion and the table in the same change. #9989 happened because
  a test asserted the stale fail-closed policy (`!persistent_took_live`) after
  #9729 had intentionally widened the mirror — the model-equality invariant was
  intact the whole time.
  - **Deleting `restore_module_globals` / `module_globals` /
    `extract_module_globals_from_vm`** (needs LV5b to cover every module kind on the
    live path; LV6 removes them with the rest of the Legacy path).

### LV6 — FLIPPED (default = Persistent; production goal MET)

LV6 is the epic's terminal slice — flip the production REPL default from `Legacy`
to `Persistent`. **The flip has LANDED: the production REPL default is now
`EvalModel::Persistent`.** The differential-completeness audit that the flip's
safety rests on was performed first (recorded below as history); it found two
real `Legacy≢Persistent` divergences — NOT in the differential corpus but caught
by a full `cargo nextest run` — that originally blocked the flip. **Both blockers
are now fixed and merged** (#9786 via PR #9795 — NamedTuple arms on
`TupleUnpack`/splat/iterate; #9787 via PR #9812 — reachable-only struct-heap
transplant compaction + VM re-read + `Rc` dedup), so the flip is now safe and the
default was flipped.

- **The flip (this slice).** `EvalModel`'s `#[default]` moved from `Legacy` to
  `Persistent` in `repl/session.rs`. Every `REPLSession::new(...)` caller — the
  sjulia CLI REPL (`run_repl`), the C ABI `repl_session_new` (iOS / Flutter),
  the web bindings, and every integration-test binary that uses the default —
  now evaluates on the Persistent model. **The `EvalModel` switch and the Legacy
  variant are kept INTACT**: the differential harness (`repl_differential_9199_tests`)
  pins one session to `Legacy` (the oracle) and one to `Persistent` to prove
  parity; switch removal + Legacy-path deletion is **LV6b (Issue #9784)**, gated
  behind this flip.
- **Verification (the decisive full-suite gate).** Under the flipped default:
  a full `cargo nextest run --release` (default features) is GREEN (4835/4835,
  0 failed); a full `cargo nextest run --release --features repl` is GREEN
  (4877/4877, 0 failed) — the run that previously surfaced the two blockers now
  passes them (`test_repl_namedtuple_persistence`,
  `repl_session_memory_stats_stay_bounded_over_1000_iterations_issue_8625`, and
  the new `repl_persistent_struct_heap_stays_bounded_over_1000_iterations_issue_9787`
  all PASS); the differential harness is 8/8 green; the AoT gate
  (`test_aot.sh`) is green; clippy (default/repl/aot) `-D warnings` is clean;
  `check_ffi_abi_version.sh` is green with no bump. A product-surface smoke of the
  real sjulia REPL binary (multi-statement eval, cross-eval global persistence +
  mutation, struct define/use, function redefinition, NamedTuple cross-eval
  destructure, module define+mutate+observe, a 200-iteration struct-allocating
  loop) matches upstream `julia` on every row.
- **The epic's production goal is MET.** The persistent-VM REPL model is now the
  SHIPPED default; #9199 can close once the lead confirms, with retirement
  (LV6b, #9784) continuing as follow-up.

#### Audit history (why the flip was originally deferred)

The differential-completeness audit — performed before the flip — is the "STOP
on a real divergence" path from the migration's compatibility rules: flipping
over a known divergence is worse than deferring, so the flip waited for the two
blockers to be fixed.

- **What the audit did (and the corpus rows it added).** The differential harness
  (`tests/repl_differential_9199_tests.rs`) proves `Legacy ≡ Persistent` — same
  input sequence through a `Legacy` and a `Persistent` session, observational
  equality per step — over the whole reused `repl_session/*.toml` corpora **and**
  the migration-seam corpus (8/8 green). The audit added `lv6_*` rows for two
  product-surface constructs the ADR flagged as historically bug-prone but NOT yet
  in the corpus — a **tuple global** assigned once and read/indexed across later
  evals (#8243) and a **multi-statement single input** — both verified
  `Legacy ≡ Persistent` AND matching upstream `julia` (`(1,2,3)`/`2`/`4`;
  `30`/`10`/`40`). Those pins stay.
- **The two blockers (found by the FULL suite, not the corpus — this is why the
  full-suite audit is mandatory).** Under a Persistent default:
  1. **#9786 — NamedTuple cross-eval destructure.** `F = (U=…, S=…, V=…)` in one
     eval then `U, S, V = F` in a later eval fails with
     `InternalError: Expected Tuple, got NamedTuple(...)` (works in `Legacy` and in
     a single program and in upstream `julia`). Root: `Instr::TupleUnpack` has no
     `Value::NamedTuple` arm; under Persistent `F` is a value-carried runtime
     NamedTuple. Regresses the common `F = svd(A); U,S,V = F` workflow. Reproduced
     by `repl::tests::test_repl_namedtuple_persistence` under a Persistent default.
  2. **#9787 — unbounded `struct_heap` growth over long sessions.** A single
     session's `struct_heap` grows linearly with eval count (measured 188 → 19188
     over 1000 evals) instead of staying bounded, because the Persistent path
     transplants the ENTIRE prior `last_struct_heap` into each fresh VM
     (`session.rs:745`) and saves the whole grown heap back (`session.rs:764`) — no
     reachable-only compaction. `Legacy` rebuilds each eval's heap from literal
     re-injection, so it stays bounded. Regresses the #8625 memory-boundedness
     guarantee for long iOS/web sessions. Reproduced by
     `repl_session_memory_stats_stay_bounded_over_1000_iterations_issue_8625` under
     a Persistent default.
  Both blockers were `bug`-filed and fixed on their own PRs before this flip
  re-attempt (favoring "a correct, well-verified increment over a broad risky
  one" — the architectural #9787 compaction did not ride along on the flip PR).
  They are now regression pins under the Persistent default (the two boundedness
  tests + the namedtuple-persistence test all run on the default model).
- **What remains after the flip.** The Legacy path + retirement-list symbols +
  `EvalModel` switch are removed LAST, in **LV6b (Issue #9784)**, because the
  differential harness needs `Legacy` as its oracle (side A = Legacy). LV6b first
  re-points the harness at a golden / upstream-`julia` oracle, then deletes the
  Legacy path.
- **C ABI:** unchanged by the flip — `check_ffi_abi_version.sh` green, no
  `SUBSET_VM_ABI_VERSION` bump. The `EvalModel` switch is Rust-internal behind the
  opaque `void* session` handle; flipping its default changes no C signature.
- **Incidental gap filed (Discovery Rule).** The product-surface smoke also
  surfaced a **pre-existing, model-agnostic** lowering gap unrelated to the flip:
  a short-form function body with a `global` declaration (`f() = (global x = …)`)
  fails to lower (`UnsupportedExpression("global_declaration")`) while the
  long-form `function f() … global x = … end` works — it reproduces identically
  under `sjulia -e` (no eval model involved) and works in upstream `julia`. Filed
  as **#9817** (`unsupported-feature`); it fails the same way under Legacy, so it
  does not block the flip.

---

## Compatibility rules DURING migration (coexistence → differential → cutover)

The migration is **not** a big-bang rewrite. Each slice adds the new path behind
a flag and proves equivalence against the old path before the old path is deleted.

1. **Old and new coexist behind a session-internal switch.** Introduce an
   internal `EvalModel` selector on `REPLSession` (default = `Legacy`). The new
   persistent path is `Persistent`. The switch is a Rust-internal detail — it is
   **never** exposed across the C ABI (see next section). This keeps `main`
   shippable at every slice.
2. **Differential comparison is the gate.** The S2 harness drives the **same
   input sequence** through a `Legacy` session and a `Persistent` session and
   asserts equal observations per step (displayed output, `typeof`/type tag,
   stdout, error class). A slice may not delete the legacy path for a construct
   until that construct is differentially green on the new path. Today both sides
   are `Legacy`, so the harness is a self-comparison that **pins the current
   behavior surface** (and guards determinism); as slices flip the second side to
   `Persistent`, the same corpus catches every divergence.
3. **Per-construct cutover, not global.** Cut over category by category (scalars,
   redefinition, structs, closures, using/import, errors, display, reset), each
   gated on its differential rows. A construct that legitimately *should* change
   output under the new model (there should be very few — the point is parity) is
   an explicit, reviewed corpus edit with an Issue link, never a silent drop.
4. **Retire only after green.** When every corpus row for the epic is green on
   `Persistent` and a full `cargo nextest run --release` passes, flip the default
   to `Persistent`, then delete the `Legacy` path and the retirement-list symbols
   (below) in a dedicated cleanup slice. The `EvalModel` switch is removed last.
5. **No regression in the existing REPL parity matrix** (`repl_session_fixture_tests.rs`
   #8714, `sample_source_repl_parity_tests.rs` #9156) at any slice.

---

## C ABI / iOS boundary — keep the surface STABLE through the migration

**Decision: the entire migration stays behind the opaque `void* session` handle;
no C ABI signature changes, so no `SUBSET_VM_ABI_VERSION` bump is required for
S3–S6.**

The REPL C ABI (`subset_julia_vm_ffi/src/repl_ffi.rs`, `include/subset_vm.h:212-259`)
is already model-agnostic:

- `repl_session_new(seed) -> void*` — returns an **opaque** pointer. Callers
  (`REPLSessionManager.swift`, `mobile/lib/ffi/vm_bridge.dart`) never see the
  struct layout, so `REPLSession` may gain/lose/reorder fields (e.g. hold a live
  `Vm`) with zero ABI impact.
- `repl_session_eval(void*, const char*) -> CREPLResult*` — input is a string,
  output is `CREPLResult` (a `#[repr(C)]` struct of owned C strings: `success`,
  `output`, `value`, `error`, `artifact_mime`, `artifact_data`). None of these
  reference the eval model. As long as the persistent path produces the same
  display/output/error/artifact **strings** (which the S2 differential harness
  enforces), `CREPLResult` is byte-for-byte compatible.
- `repl_session_reset(void*)` / `repl_session_free(void*)` — reset gains its
  correct "drop VM, rebuild fresh" semantics internally; the signature is
  unchanged. `free` still drops the boxed session.

**Design constraints to preserve stability:**

- Do **not** add new C ABI functions for the migration (no
  `repl_session_set_model`, no snapshot API). The `EvalModel` switch is internal;
  tests select it through a Rust-only test hook, not a C entry point.
- Do **not** change `CREPLResult`'s fields or their meaning. If S3+ ever needs to
  surface new host data (it should not for this epic), that is a **separate**,
  independently-versioned ABI change following the `docs/vm/CHECKLISTS.md`
  "ABI Change Checklist" — not folded into this migration.
- The single-thread contract (`owner_thread` debug guard, #9056/#8214/#8675) is
  **strengthened** by a live VM (more `Rc`/`RefCell` state), not weakened. The
  live-VM session remains `!Send`/`!Sync`; the raw-pointer ABI still requires all
  calls on the owner thread. No change to that documented contract.

Net: `kSubsetVMABIVersion` (Swift) / `SUBSET_VM_C_ABI_VERSION` (Rust) stay at
their current value across S3–S6. `check_ffi_abi_version.sh` must stay green
without a bump; if a slice trips it, that slice has leaked model state into the
ABI and must be reworked.

---

## Interaction with `PROGRAM_CACHE`

`PROGRAM_CACHE` (`compile/cache.rs:1016`, thread-local `HashMap<u64, CompiledProgram>`
keyed by the content hash of the final merged program) is an artifact of the
recompile model: it caches whole accumulated programs. In a growing session its
hit rate is ~0 because every new definition changes the hash.

- **Through S3–S4** it is unaffected (the accumulated-program recompile still
  runs; globals/modules just stop round-tripping through expr/const rewrites).
- **At S5** the cache's role shrinks: eval compiles only the input delta, so the
  cache key becomes the input's own hash (small, stable, genuinely reusable —
  re-running the same line is a real hit). The prelude/Base embedded caches
  (`SJULIA_PRELUDE_PROGRAM_CACHE`, `SJULIA_BASE_CACHE`) are unrelated and stay as
  is.
- The per-VM `RuntimeCompileContext` incremental-compile path is the machinery
  S5 reuses to compile a delta against the live VM.

No cache format or key change is needed before S5; when S5 lands, update
[CACHE_ARCHITECTURE.md](./CACHE_ARCHITECTURE.md) to record the key change.

---

## Invalidation dependency (#8553 / #8554, foundation #9197)

The persistent model's correctness for **redefinition** rests on precise
invalidation, already built:

- **#8553 / #8554**: `MethodKey` / `SpecializationKey` + `WorldRange` + backedge
  invalidation. When S6 inserts a redefined method into the live method table, it
  bumps the world and invalidates exactly the dependent `CodeInstance`s via
  backedges — the upstream behavior — instead of the current "next eval recompiles
  the whole accumulated program, so the new body is naturally picked up."
- **#9197** (interned type IDs + typemap type index) is the type-identity
  foundation: reliable per-VM world/backedge invalidation needs a stable,
  interned notion of type identity for the typemap index, rather than string
  type-names / unverified hashes. S6 should land on top of #9197 so the typemap
  keying is sound.

Until S6, S3–S5 keep the (correct, if slow) full-recompile-for-visibility
behavior; S6 is the slice that finally makes redefinition a world-counter fact.

---

## Retirement list (Issue #9784 progress)

**Retired on 2026-07-11:** the `EvalModel` enum and its accessors, every Legacy
compile/carry branch, Legacy test/benchmark comparisons, the Legacy side of the
differential harness, and the redundant `struct_instances` literal mirror. The
harness now runs the sole production session against recorded upstream-reviewed
goldens and separately compares two independent sessions for determinism. Struct
globals retain one authoritative `StructRef` in `REPLGlobals`; full-recompile
fallbacks reconstruct it once from `last_struct_heap` instead of replaying a
second cached `Literal::Struct` assignment.

**Runtime-error recovery landed on 2026-07-17:** a snapshot-stable live delta
that raises an unhandled, Julia-catchable runtime error no longer discards its VM. The VM-owned
`recover_repl_toplevel_after_error` operation unwinds invocation-local frames,
stack entries, handlers, tasks, transient roots, output/error state, display
state, and RNG while preserving frame 0, heap identity, modules, installed
definitions, method worlds, and dispatch caches. Assignments and module
mutations completed before the exception remain authoritative, matching Julia's
single-module toplevel evaluation. Until the remaining fresh fallback is
retired, the error branch also synchronizes those completed mutations into the
transitional mirrors so a later fallback cannot resurrect pre-error values. The
synchronization includes globals created indirectly inside a called function,
the separate host-facing `ans` mirror, and slot migration when a later compiled
delta first gives a dynamic binding a static frame-0 slot. Redirected streams are
unwound in LIFO order before the VM is parked.
**Main-source method live activation completed on 2026-07-18:**
Function-definition deltas now commit the exact source-ordered prefix reached
before a runtime error. `DefineEvalFunction` is the sole publication point;
unreached method bodies remain dormant behind their method-world fence, and the
compiler snapshot/replay mirrors retain only the same reached prefix. Reflection,
direct and dynamic calls, forward references, and IR inlining therefore cannot
expose a later definition early. The same transaction now covers ordinary,
`where`, keyword, vararg, and combined source methods, including extensions,
same-signature replacements, marker-specific transitive caller refreshes, and
their specialization rows. Keyword calls with a later dormant replacement use
world-aware function-value dispatch, so a caller compiled before that marker
selects the reached prior method instead of targeting the future body directly.
Brand-new concrete-struct deltas likewise retain their exact reached prefix;
abstract, primitive, enum, parametric, inner-constructor, and redefined type
families still take the fresh fallback. A source-stable call that changes the
definition world indirectly through runtime `@eval` is detected by a pre/post VM
definition fingerprint and drops for the same reason. Committing those remaining
type/indirect worlds in source order is a later #9784 transaction slice rather
than an inferred partial commit.
Host cancellation and VM-internal invariant failures remain on the drop path.

**Marker-less HOF helper live installation completed on 2026-07-19:** ordinary
top-level lambda/HOF, do-block, generator-body, and filtered-generator predicate
helpers now install beside source methods on the held VM. Function-table position
is not publication authority: the ordered `ReplDefinitionActivation` set names
every source primary and refresh member, those members remain world-gated until
their source marker, and every appended non-member helper is visible immediately
at world 1. Error recovery validates and projects the exact reached activation
prefix while retaining helper bodies for index alignment. Compiler snapshots add
only source primaries to Julia-visible generic/method-source registries, so
`__lambda_*`, `__do_block_*`, `__gen_body_*`, and `__gen_pred_*` cannot leak into
dispatch or reflection as named generics. Mixed helper+method inputs and helpers
on both sides of a thrown error remain live with zero fresh VM builds.

**Still required by the production Persistent model:** the symbols below are no
longer a second eval model, but they remain on fresh full-recompile fallbacks.
Deleting them today would lose heap-backed globals or state/definitions when a
session transitions between live append and a fallback. Current exclusions
include package modules, inner `using`/`import`, module-level macros/type aliases,
`baremodule`, non-mirrorable module bindings, parametric/inner-constructor/redefined
type families, Base/preload-owned generic extensions, future helper shapes whose
complete target/alignment surface fails structural extraction, and opaque runtime
`eval` (see the LV3b remainder, LV4b, LV5b, and Issue #9723). Therefore
Issue #9784 remains open.

- `repl/converters.rs`: `value_to_init_expr` (+ `value_to_init_expr_inner`) and
  the value→literal reverse-conversion path used only for re-injection. The old
  general `extract_assigned_variables` persistence scan is retired; only the
  target-aware `potential_rebindings_of` pre-run check remains for the narrow
  struct/self-rebinding seed hazard, and its result never commits state.
- `repl/session.rs`: `inject_globals`, `merge_definitions`,
  `restore_module_globals`, `extract_globals_from_vm`,
  `extract_module_globals_from_vm`, `store_definitions` (folded into differential
  apply), `seed_persisted_globals` usage for globals.
- `REPLSession` fields: `global_types`, `global_struct_names`, `module_globals`,
  the definition `(Vec, HashMap index)` pairs (become live
  VM registries), and `last_struct_heap` transplanting.
- The per-eval global re-injection, per-eval module-body re-execution, and
  accumulated-source recompile as concepts.

`REPLGlobals`'s per-type maps and the `value_to_literal` completeness contract
(#3298) may survive as a *display/introspection* helper, but stop being the
persistence mechanism.

---

## REPL state authority and continuation closure (Issue #10262)

The production model now assigns one authority to each kind of REPL state; it
does not infer persistence from a value's display form or from CLI keywords:

| Concern | Authority / replacement rule | Regression gate |
|---|---|---|
| Runtime values and object identity | The held persistent `Vm`; successful eval commits the extracted reachable globals, while `reset()` constructs fresh-session state | `reset_is_observationally_equivalent_to_fresh_9199`, session boundedness tests |
| Top-level definitions used by full-recompile fallbacks | `REPLSession`'s typed definition registry (`functions`, macros, structs, abstract/primitive/enum types, modules), merged by stable signature/name; current input replaces the same key | `test_repl_abstract_type_persists_across_evals_9701`, `test_repl_primitive_type_and_enum_persist_across_evals_9701` |
| Module redefinition | `newly_defined_module_paths` makes the current definition authoritative and excludes its old globals from restoration; ordinary module references keep the live module state | `test_repl_module_redefinition_resets_const_state_10232` |
| Unqualified names inside module methods | Module bindings/consts precede imported/Base function fallback | `modules_module_const_shadows_base_function_10234` |
| Interactive multiline completeness | Pure-Rust parser recovery diagnostics. Only appendable EOF states (`UnexpectedEof`, EOF token expectation, unterminated string/character/comment, unclosed bracket) request another line; ordinary syntax errors are submitted immediately | parser `incomplete_input_classification_uses_parser_recovery_state_issue_10262` + `sjulia` `repl_incomplete_*` tests |

The last row retires the manual `keywords_open` table that caused #10235. A new
Julia block form now reaches the same parser used for execution, so grammar
growth cannot require a second CLI opener list. This also closes prevention
Issue #10862, including negative syntax cases and `end; # comment` adjacency.

Script/REPL product parity remains covered by
`samples_produce_identical_output_as_script_and_repl_9156` and the migration
seam corpus. The retained typed definition vectors are an implementation of the
logical registry contract for fallback compilation; they are not an alternate
runtime-value store.

---

## Exit criteria (epic-level acceptance)

> **LV6/LV6b status (2026-07-11):** the production REPL has one persistent model
> and no selector. The two audit blockers are fixed
> (#9786 NamedTuple cross-eval destructure via PR #9795; #9787 unbounded
> `struct_heap` growth via PR #9812), the differential harness is 8/8 green, and
> both a full default-features `cargo nextest run --release` (4835/4835) AND a
> full `--features repl` run (4877/4877) are green under the flipped default. The
> epic's production goal (persistent-VM REPL as the shipped default) is **MET**;
> #9199 is closed. Issue #9784 remains open for the full-recompile fallback
> retirement list above; the golden harness no longer needs Legacy as an oracle.

- [x] **ADR merged** (this document): target shape + stages fixed. *(S1)*
- [x] **Differential harness merged and green** on current `main` as a
      self-comparison (S2), and green as `Legacy` vs `Persistent` — 8/8 green at
      the LV6 audit, including the `lv6_*` tuple-global (#8243) + multi-statement
      product-surface rows added by the completeness audit.
- [~] **eval cost is independent of session length** — **MET for the LV2 + LV3 +
      LV4 subsets**, not yet epic-wide. `benches/repl_input_delta_9199.rs` prints
      three curves: `[A]` expression / global-(re)assignment deltas (LV2) —
      `Persistent` COMPILE ~**FLAT** (~1.3x N=0→80 vs Legacy ~23x), vm-build **0**;
      `[B]` brand-new generic FUNCTION DEFINITION deltas (LV3) — `Persistent`
      COMPILE ~**FLAT** (~1.1x N=0→80 vs Legacy ~11x), vm-build **0**; and `[C]`
      brand-new non-parametric STRUCT DEFINITION deltas (LV4) — `Persistent` COMPILE
      ~**FLAT** (~1.1x N=0→80 vs Legacy ~20x), vm-build **0**. LV4 compiles only the
      struct delta (`repl_relocatable_delta_compile` → `AppendableDelta.new_struct_defs`)
      and reserves an aligned type-registry tail in the live VM
      (`Vm::reserve_appended_types`), activating each entry only when its
      source-ordered `DefineEvalStruct` marker executes, instead of recompiling the
      accumulated program. Ordinary lambda/HOF, do-block, and generator helpers
      are also installed live using activation-index membership rather than a
      source-function prefix count. A delta that extends a Base/preload-owned
      generic, contains an opaque future helper shape, references an unsupported
      user-module surface (LV3b/LV5b), or defines a parametric / inner-ctor /
      redefined type (LV4b), still falls back to the full recompile path
      (O(session)), so the epic criterion is not yet fully met. Main-owned ordinary,
      `where`, keyword, vararg, and combined source methods — including extensions
      and replacements — now remain on the live path with vm-build zero.
      See §"LV2 — LANDED", §"LV3 — LANDED", and §"LV4 — LANDED".
- [x] **#9182 / #9193 hold structurally**: a top-level `let`'s new locals vanish
      with the frame (no persistence heuristic can leak them), and `reset()` drops
      **all** state because reset = fresh session (no field-by-field clearing;
      `last_vm_memory_stats` and the default `InteractiveUtils` import included).
      Pinned in BOTH models by `index_key_hard_scope_let_does_not_corrupt_frame0_global_9199`
      + `reset_is_observationally_equivalent_to_fresh_9199`.
- [x] **The general syntax-based global persistence scan is deleted.**
      `extract_assigned_variables`, `extract_declared_globals`, and their recursive
      / timing-macro helpers no longer exist. Executed VM stores are authoritative:
      `repl_written_globals ∩ main_scope_names` admits ordinary module-scope writes,
      while the separate `StoreGlobalAny` trace admits unqualified explicit
      `global` writes from hard scopes and called functions. Qualified stores
      remain owned by `module_globals` and never enter Main's value mirror.
      Definition/alias publication is filtered separately. The retained
      target-aware `potential_rebindings_of` scan exhaustively visits executable
      IR expression positions as compile preparation only for a struct-bearing
      full rebuild that may self-rebind a prior global; it is not a persistence
      authority (Issue #9784).
- [ ] **`value_to_init_expr` / `inject_globals` / `restore_module_globals` are
      deleted** from the tree. These helpers remain live for the production
      Persistent model's fresh full-recompile fallbacks. Delete them only after
      the remaining ownership/extraction remainder plus LV4b/LV5b shapes no longer
      require reconstructed globals, accumulated definitions, or the module-state
      mirror (Issue #9784).
- [x] **REPL semantic parity matrix (#8971/#8980, #8714, #9156) has no
      regression** old-vs-new — **MET under the flipped Persistent default.** The
      two LV6-audit regressions are fixed (**#9786** NamedTuple cross-eval
      destructure via PR #9795; **#9787** `struct_heap` unbounded growth via
      PR #9812) and pinned by regression tests that now run on the default model
      (`test_repl_namedtuple_persistence`, the two `..._issue_8625` / `..._issue_9787`
      boundedness tests). A full `--features repl` run is green (4877/4877,
      **zero** failures — the #9586 dump-bytecode test (`..._issue_8147`) also
      passes: it is a `--dump-bytecode` CLI/script-mode test the eval model
      cannot affect, and Issue #9586 is CLOSED on main).
- [x] **C ABI unchanged**: `check_ffi_abi_version.sh` green with no version bump
      (`SUBSET_VM_ABI_VERSION=2`); `CREPLResult` layout and the four
      `repl_session_*` signatures untouched.
- [x] **`reset()` == `new()`**: `reset_is_observationally_equivalent_to_fresh_9199`
      asserts a reset session is observationally identical to a fresh one
      (including auto-imports), verified in both models.

---

## Related work

- **This week's fix burst (all the same follow-the-heuristic family)**: #9156,
  #9157, #9172, #9173, #8976, #8977; closed symptom-level fixes #9182, #9193.
- **Historical members**: #8260 (value-carry, the expressiveness-ceiling proof),
  #5296 (`module_globals` fakery), #8243 (tuple globals), #8452 (eval world-age).
- **Reused foundation**: #8553 / #8554 (WorldRange / backedge invalidation),
  #9197 (interned type IDs / typemap index), the per-VM `RuntimeCompileContext`
  incremental path, and the REPL parity harnesses #8971 / #8980 / #8714 / #9156.
- **Script-mode dual**: #9400 (top-level redefinition wins retroactively) — same
  missing-world-age root, fixed by S6.
- **Upstream references**: `julia/src/toplevel.c` (`jl_toplevel_eval_flex`,
  statement-boundary world capture), binding partition (`julia.h`), world counter
  (`julia/src/gf.c`).

---

## Considered alternatives

- **Keep the recompile model, keep patching heuristics.** Rejected: this is the
  status quo that produced ≥8 same-root bugs in three days and cannot express
  identity/IO/self-referential values (#8260) or make eval cost sub-linear.
- **Serialize/snapshot session state to a blob and restore it.** Rejected: still
  a value→representation→value round-trip with the same expressiveness ceiling,
  plus a new serialization surface to version. The live-VM model has no
  round-trip.
- **Big-bang rewrite of `eval`.** Rejected: no safety net, unshippable `main`
  between start and finish. The coexistence + differential + per-construct cutover
  plan keeps every intermediate state green and shippable.
