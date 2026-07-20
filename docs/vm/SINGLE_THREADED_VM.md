# Single-Threaded VM Decision Record (Issue #8649)

*Last updated: 2026-07-13 (VM task continuations, Issue #10349)*

## Decision

SubsetJuliaVM VM instances are single-threaded runtime objects. A `Vm`,
`REPLSession`, or FFI session handle has one host owner at a time and must not be
called concurrently or moved between host threads as a shared mutable runtime.

Concurrency belongs above the VM boundary:

- run multiple independent VM/session instances on separate host workers when
  parallelism is needed;
- serialize all calls that touch a given VM/session handle;
- communicate between VM instances through host-owned messages, files, or other
  explicit data exchange, not by sharing VM internals.

This is an architectural decision, not just a current implementation accident.
The no-JIT iOS runtime should optimize for predictable single-threaded execution
and cheap value movement before paying synchronization costs for Julia-level
threading.

## Context

SubsetJuliaVM is a static Julia-subset runtime for iOS and WebAssembly. The VM
does not use a tracing GC, OS-thread scheduler, or JIT-generated synchronization
points. It does have a VM-local cooperative Task scheduler: continuations switch
frame/stack segments inside the single host dispatch loop and never transfer a
VM/session between threads. The current Rust API already encodes exclusive execution through
`&mut self` on `Vm::run()`-style paths and `REPLSession::eval`.

The C ABI erases Rust's `!Send` and `!Sync` type information. For example,
`repl_session_eval` turns an opaque raw pointer back into `&mut REPLSession`.
That is only sound when the host treats the pointer as single-owner state. The
iOS app therefore serializes access to a session; Issue #8675 records the
follow-up work to make the FFI boundary comments and host-thread constraints
discoverable in headers and app documentation.

## Rationale

### Hot Runtime Values Use `Rc` And `RefCell`

`Value` intentionally uses single-threaded carriers such as `Rc`, `RefCell`,
and `Rc<RefCell<_>>` for mutable Julia containers and shared runtime metadata.
These carriers make clones cheap and keep interior mutability explicit without
atomic reference counts or locks on the VM hot path.

Replacing them with `Arc`, `Mutex`, or `RwLock` throughout the runtime would tax
ordinary scalar loops, dispatch, array/tuple/struct movement, formatting, and
higher-order-function execution even when no user code asks for threads. That
is the wrong default for no-JIT iOS execution.

### `struct_heap` Is A Single-Owner Heap

Mutable structs and several wrapper values use a VM-local `struct_heap`
(`Vec<StructInstance>`) and refer to heap slots by index. The VM mutates this
heap during execution and compacts it only at explicit safe points.

Concurrent mutation would require a different ownership model: stable handles
or generations, synchronization around every allocation/read, and a compaction
protocol that can update outstanding references across threads. That is a GC
and handle-table design, not a local refactor.

### Thread-Local Caches Are Part Of The Current Contract

Several registries and caches are intentionally thread-local. Promotion rules,
quoted-expression conversion helpers, Base/prelude cache state, and dispatch
support code can assume a single execution lane for the active VM/session.

Sharing one VM across threads would make those caches either incoherent or
require cross-thread invalidation. Keeping each VM instance on one host owner
keeps the cache model simple and predictable.

## Impact

- New runtime code may use single-threaded Rust ownership (`Rc`, `RefCell`,
  `Cell`, `thread_local!`) when the value or cache is VM-local.
- New code must not promise `Send` or `Sync` for `Vm`, `REPLSession`, `Value`,
  VM frames, `struct_heap` entries, or FFI session handles unless a separate
  design proves the full ownership boundary.
- Host adapters may run VM work off the UI thread, but each VM/session handle
  needs one serialized owner lane. Parallel host work should use separate VM
  instances.
- Julia Tasks, Channels, Conditions, timers, and `@sync` are cooperative within
  one VM owner lane (Issue #10349). They add concurrency/interleaving, not
  parallel execution or `Send`/`Sync` requirements.
- `Base.Threads` exposes the accepted single-thread compatibility shim
  (`nthreads() == threadid() == 1`, Issue #8991); it is not shared-memory
  threading.
- FFI and app-facing documentation must state the handle rule: create, call,
  reset, and free a live VM/session handle on its owner thread.

## Alternatives Considered

### Convert The Runtime To `Arc` And Locks

This would make many types technically transferable across threads, but it
would not by itself make the VM semantically thread-safe. Dispatch caches,
`struct_heap` indices, safe-point compaction, output buffers, frame pools,
runtime globals, and user-visible mutation would still need higher-level
coordination. The cost would hit every program before the design provides real
Julia threading semantics.

Decision: reject as a default architecture.

### One Global VM With A Coarse Lock

A global lock would avoid data races but would also serialize all execution and
still expose a misleading shared-threading model to hosts. It gives most of the
complexity of shared access without meaningful parallel speedup.

Decision: reject.

### Multiple VMs Plus Message Passing

Independent VM instances on separate host workers match the current ownership
model. They can run in parallel as long as values cross the boundary through
explicit serialization or host-managed messages.

Decision: preferred future concurrency shape.

### `Threads.nthreads() == 1` Compatibility Shim

Some Julia packages only check thread availability or branch on
`Threads.nthreads()`. Returning `1` and rejecting thread creation can be
upstream-compatible for single-threaded Julia configurations.

Decision: acceptable as a future compatibility feature, tracked separately from
multi-threaded execution.

## Cost To Reverse

Reversing this decision requires a new runtime ownership design, not a mechanical
rename:

- replace or isolate `Rc`/`RefCell` carriers without regressing hot-path value
  movement;
- redesign `struct_heap` references so compaction and mutation are safe across
  threads;
- define cross-thread cache invalidation for thread-local registries and
  dispatch state;
- audit every FFI export that reconstructs `&mut` from a raw pointer;
- specify Julia-level memory, task, exception, output, and cancellation
  semantics under parallel execution;
- re-run host, iOS Simulator, and WebAssembly performance baselines.

The likely viable reversal is still not "make one VM shared"; it is a
multi-instance design with explicit message passing and only small, audited
shared runtime tables.

## Thaw Conditions

Revisit this ADR only if at least one of these becomes true:

- a target package required by the iOS/WebAssembly roadmap cannot be supported
  with single-thread-compatible shims and has a concrete business/user need for
  Julia shared-memory threading;
- measurements show that a proposed ownership redesign preserves or improves
  VM-only performance on iOS and WebAssembly while passing the full fixture
  suite;
- the project adopts a real GC/handle-table design that already solves
  cross-thread `struct_heap` references and safe-point compaction;
- Apple/WebAssembly platform constraints change enough that a different
  concurrency/runtime architecture is justified.

Until then, new work should assume VM instances are single-threaded, keep each
live handle on one owner thread, and put concurrency at the host orchestration
layer.

## Future Threads Request Template

When a future request involves `Base.Threads`, `@threads`, `@spawn`,
`threadid`, `nthreads`, channels/tasks used for parallel execution, or package
code that assumes Julia shared-memory threading:

1. Link this ADR and state that sjulia VM instances are intentionally
   single-threaded; host-level parallelism uses multiple VM instances plus
   explicit message passing.
2. If the construct runs in upstream Julia but fails in sjulia, file an
   `unsupported-feature` Issue before adding any workaround. Include a minimal
   example and a julia-vs-sjulia output table.
3. Classify the request as either a single-thread-compatible shim
   (`Threads.nthreads() == 1`, feature detection, deterministic serial fallback)
   or true shared-memory threading. Shims may proceed under their own Issue;
   true threading requires reopening this ADR under the thaw conditions above.
