# VM Memory Management

Issue #8453 tracks the interpreter VM memory policy for long-running hosts such
as iOS apps and REPL sessions.

## Current Strategy

The interpreter uses a conservative safe-point mark/compact pass for
`Vm::struct_heap`. Mutable structs keep Julia reference identity as
`Value::StructRef(heap_index)`, so the VM cannot drop an entry while arbitrary
callee frames are live. The compactor therefore only runs at VM safe points:
after normal `Vm::run()` exit, or when a host calls
`Vm::compact_struct_heap_at_safe_point()` and the VM has no live callee frame,
exception handler, HOF continuation, sprint state, generated-eval continuation,
or nested eval dispatch.

At a safe point the VM marks from stack values, the top-level frame, and the
optional return value, follows nested `StructRef` fields through heap entries,
rebuilds the heap densely, and rewrites retained indices. Dead mutable struct
entries are dropped immediately and the remaining heap contains only reachable
entries.

This is option B from #8453, scoped to stop-the-world safe points. It matches
the single-threaded no-JIT iOS constraint and avoids per-assignment reference
count traffic in hot interpreter paths.

### Cross-eval transplant compaction (Issue #9787)

The safe-point pass above compacts a *single* VM's heap. The REPL under the
`Persistent` eval model (Issue #9199) is a second, cross-eval axis: each eval
builds a fresh VM and carries prior globals forward by transplanting the prior
eval's struct heap into it (`Vm::seed_persisted_globals`, so every carried
`Value::StructRef(idx)` stays valid). `REPLSession` keeps that heap in
`last_struct_heap` and re-saves it after each eval.

Transplanting the heap **verbatim** grew it without bound: `last_struct_heap`
accumulates every eval's structs, but only the seed globals — the values carried
directly into the fresh VM's binding table — actually reference the transplanted
region (globals reconstructed as init statements rebuild self-contained literals
into fresh indices, and struct instances stored for reconstruction are literals,
not heap refs). Over a long session the dead accumulation dominated (~188 →
~19188 over 1000 evals of a small struct-allocating program), violating the
#8625 boundedness guarantee and blocking the LV6 default flip.

`reachable_compacted_struct_heap` (`vm/state.rs`) closes this at the transplant
boundary. It is the cross-eval analogue of the safe-point pass: instead of
marking from a VM's own frame/stack roots, it marks from the **seed globals** (the
references into the transplanted region), using the same exhaustive
`mark_value_struct_refs` / `remap_value_struct_refs` walkers (nested struct
fields, arrays/tuples/named-tuples, `Pairs`, `Ref`, generators, `Expr` args,
`Memory`, closure captures, …). It transplants only the reachable structs,
rewriting every `StructRef` in the seed globals and the retained structs' fields
to the new dense indices, and returns the compacted heap. The work is
O(reachable structs + roots), never O(session), so `last_struct_heap` stays at its
per-eval steady state across an unbounded session. Scalar-only seed globals
(`s = 0`, `ans`) reach no struct, so the entire prior heap is reclaimed.

**Keeping the session's OWN cached indices consistent.** `REPLSession` caches a
struct global's heap index in `self.globals`
(`REPLGlobals::struct_ref_vars`, and nested inside tuple / closure / other
values). Compaction moves a carried struct's index (e.g. `33 → 1`); the seed copy
handed to the VM is remapped, but `self.globals` must agree with the post-run
heap or a later eval reads the wrong struct (the #9787 corruption: an `ODEProblem`
whose `solve` dispatch stopped matching after the VM appended fresh structs over
the vacated slot). Rather than remap `self.globals` in place before the run —
which is **not transactional** if the run errors, and **double-remaps shared `Rc`
containers** aliased between two globals — the fix makes the **VM the source of
truth**: `extract_globals_from_vm` re-reads every CARRIED global (listed in
`prior_global_names`, now populated under BOTH eval models, Issue #9787) from the
VM after a *successful* run, so `self.globals` picks up the correct post-run
indices. On an **errored** run neither `self.globals` nor `last_struct_heap` is
touched, so they stay mutually consistent.

**Alias-preserving transactional detachment (Issue #9827).** The remap rewrites
`StructRef` indices in place, while a plain `Value::clone` only bumps mutable
`Rc<RefCell<_>>` carriers (`Array`/`Memory`/`Ref`/`WeakRef`/`Expr`). Before #9827
that forced a verbatim-transplant fallback for any reachable shared carrier,
which was sound but let dead structs accumulate in those sessions. The
compactor now detaches roots and reachable heap fields as one object graph
before remapping. Per-carrier alias maps keyed by `Rc::as_ptr` allocate each
array, memory, or value cell once and reuse it at every aliasing edge; cycles and
shared-parent diamonds resolve through placeholders installed before child
traversal. Thus aliases within the candidate transplant remain aliases, but no
candidate container aliases `self.globals` or `last_struct_heap`. Remapping and
execution mutate only this detached candidate. An errored run drops it without
touching session state; a successful run is committed by re-reading the VM as
described above. There is no verbatim fallback.

**Rc-aliasing dedup in the remapper.** `remap_value_struct_refs` also threads a
**visited set keyed by `Rc::as_ptr`** so a shared
`Rc<RefCell<ArrayValue/MemoryValue/Value>>` reached twice in one pass is rewritten
exactly once (the table is not idempotent: `3→1` then `1→0`). The transplant
uses this after detachment, and the dedup remains load-bearing for the **in-VM
safe-point GC**, which remaps the live VM's own aliased arrays/refs in place and
had the same latent double-remap bug. Live-VM paths never call
`seed_persisted_globals`; their heap is bounded by the safe-point pass.

## Cache Policy — hard-cap bound (Issue #8610)

Dispatch and specialization caches are world-local VM acceleration structures.
They are still cleared on world-age changes such as `eval` activation, and a
host can inspect `Vm::memory_stats()` and call `Vm::clear_runtime_caches()`
between independent user runs.

For long-running hosts these caches are additionally bounded by an **always-on
hard cap** (Issue #8610). Each runtime cache has an entry limit; the
corresponding `enforce_*_cache_limit()` runs at the cache's insert site and,
when the cache exceeds its limit, clears it wholesale. Clearing (rather than
per-entry LRU eviction) keeps the dispatch hot paths untouched — a cleared
cache simply refills — while guaranteeing the cache cannot grow without bound
in a session that runs unboundedly many distinct call shapes. The bounded
caches are:

- dispatch-family (limit `RUNTIME_DISPATCH_CACHE_ENTRY_LIMIT`, default 4096):
  `dispatch_cache`, `binary_both_dispatch_cache`, `method_dispatch_cache`;
- specialization-family (limit `RUNTIME_SPECIALIZATION_CACHE_ENTRY_LIMIT`,
  default 4096): `specialization_cache`, `specialization_i64_cache`,
  `i64_function_cache`, `binary_method_cache`, `generated_expr_cache`.

`Vm::memory_stats()` reports the current cap for each family
(`dispatch_cache_entry_limit`, `specialization_cache_entry_limit`) and the
observability counters `cache_clears` (hard-cap clears fired) and
`cache_cleared_entries` (entries discarded), so a host can confirm the bound is
active and see how often it fires (Issue #8625).

### Host cache-cap configuration (Issue #8625)

The caps are tunable so a host can trade dispatch-cache hit rate for a smaller
steady-state footprint on a memory-constrained device:

- Per VM: `Vm::set_cache_entry_limits(dispatch, specialization)` (`None`
  restores the default).
- Process-wide, applied to every VM built afterwards — including the fresh VM
  the FFI REPL session builds per eval:
  `subset_julia_vm::vm::set_default_cache_entry_limits(dispatch, specialization)`.
- From an iOS/native host over the C ABI:
  `subset_julia_vm_set_cache_entry_limits(dispatch, specialization)` (pass `0`
  for either argument to keep the default). Call once at startup after sizing
  the budget to device memory.

The iOS app wires this at `REPLSessionManager` construction time before the
first `repl_session_new` call. The default bands are intentionally conservative:
devices below 2 GiB physical memory use `1024` entries, devices below 4 GiB use
`2048`, and devices at or above 4 GiB pass `0` to retain the VM's built-in
`4096`-entry defaults. The FFI symbol is loaded with `dlsym`, so older bundled
frameworks that predate Issue #8625 still launch; newer frameworks receive the
host cap injection.

`REPLSession::last_vm_memory_stats()` surfaces the most recent eval's
`VmMemoryStats` so a host (or the `session_boundedness_8625_tests` integration
test) can watch struct-heap and cache growth across a long session.

## Runaway Containment Axes (Issues #5969/#8214/#8685)

The iOS host has three independent runaway-program containment axes:

1. **Runaway recursion** is bounded by the interpreter call-depth guard. When a
   call would exceed the VM frame bound, the VM raises Julia-shaped
   `StackOverflowError` instead of growing until the host stack/process dies.
2. **Runaway loops** are bounded by cooperative cancellation. The UI Stop
   button calls `vm_request_cancel`; the interpreter polls at backward jumps
   and call boundaries and returns a cancellation error instead of freezing the
   app indefinitely.
3. **Runaway allocation** is bounded by the host memory budget. Known oversized
   allocations fail immediately, and incremental growth is sampled at
   safe-points/call boundaries through `memory_stats()` waterline estimates.
   Both paths surface as catchable `OutOfMemoryError`.

These axes are intentionally separate. Stack overflow and cancellation are
control-flow bounds; the memory budget is a host policy that should be sized to
device memory and injected before the VM/session is constructed.

## Host Memory-Budget Configuration (Issues #8702/#8703/#8704)

Memory budgets are opt-in for CLI/default test behavior but should be enabled
by long-running hosts:

- Per VM: `Vm::set_memory_budget_bytes(bytes)`.
- Process-wide, applied to every VM built afterwards:
  `subset_julia_vm::vm::set_default_memory_budget_bytes(bytes)`.
- From an iOS/native host over the C ABI:
  `subset_julia_vm_set_memory_budget_bytes(bytes)` (pass `0` to clear the host
  override).

The iOS app calls `subset_julia_vm_set_memory_budget_bytes` before constructing
each REPL session. The default policy uses one eighth of physical memory,
clamped to 64-256 MiB, so real devices and simulator hosts get a deterministic
budget while preserving room for normal samples. The budget is process-wide and
affects subsequently constructed VMs, including the fresh VM created for each
REPL evaluation.

`VmMemoryStats` reports both `memory_budget_bytes` and
`estimated_memory_waterline_bytes`, which lets host tests confirm that the
budget was injected and observe sampled growth without relying on allocator
internals.

Simulator measurement on 2026-07-03 (iPad (A16), Debug app, rebuilt
`SubsetVM.xcframework` with embedded Base cache) launched successfully with RSS
about 180 MiB after first frame (`ps -o rss` reported 180,180 KiB). The
AX-driven `scripts/ios_repl_e2e.sh` path could not complete in this terminal
because `osascript` stalled waiting for the Simulator window; a repeated
simulator XCTest eval probe also exposed heap corruption and is tracked
separately as Issue #9056. Until that bug is fixed, the committed iOS regression
coverage for #8625 is the host cap-band unit test plus the Rust
`session_boundedness_8625_tests` 1000-iteration memory-stats bound.

## ExprArgs cycle guard (Issue #8610)

`Value::ExprArgs` (`Rc<RefCell<ArrayValue>>`, the native-array / `Expr.args`
carrier) could form an `Rc` reference cycle if an `Expr`'s `args` came to own
its own backing array — such a cycle would never be reclaimed by reference
counting. Rather than a cycle collector, the VM **prevents** the cycle at
construction: `native_array_cycle_guard` rejects building an `ExprArgs` value
that would own the array it is being stored into (see the
`native_array_cycle_guard_rejects_*_issue_8610` tests). This keeps the
"cycles cannot be created" invariant that makes the reference-counted carrier
safe for long metaprogramming-heavy sessions.

## Host Guidance

- Call `Vm::memory_stats()` (or `REPLSession::last_vm_memory_stats()`) around
  repeated user runs to profile heap and cache growth; watch `cache_clears` to
  see the hard cap firing.
- On a memory-constrained device, lower the caps once at startup via
  `subset_julia_vm_set_cache_entry_limits` / `set_default_cache_entry_limits`.
- On iOS/native hosts, configure a memory budget before constructing REPL or
  one-shot execution VMs with `subset_julia_vm_set_memory_budget_bytes`; keep
  the budget below the platform's process-kill threshold.
- Let `Vm::run()` finish normally when possible; it attempts safe-point
  compaction before returning.
- For hosts that keep a VM alive across unrelated user programs, call
  `compact_struct_heap_at_safe_point()` and `clear_runtime_caches()` between
  programs.
- If `compact_struct_heap_at_safe_point()` returns `compacted = false`, the VM
  was not at a safe point; retry after the nested call or handler unwinds.
