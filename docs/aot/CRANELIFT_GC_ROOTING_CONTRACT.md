# Cranelift GC and Rooting Contract

**Last updated**: 2026-06-24 (Issue #7118)

This document defines the Cranelift-side contract for managed Julia values. It
is a design contract, not an enabled heap-value implementation. The current
backend must continue to reject heap-shaped or runtime `Value` programs until
the allocation hooks, stack maps, ownership model, and tagged runtime boundary
listed below are implemented.

## Value Classes

Cranelift values are classified into four groups:

| Class | Representation | Rooting rule |
|---|---|---|
| Native scalar | Cranelift scalar SSA value (`i*`, `f*`) | No GC root. |
| Native stack aggregate | Stack slot with scalar fields only | No GC root while every field is native and the value is not returned through a heap/runtime ABI. |
| Static data pointer | Pointer to immutable object-file data, such as a read-only string literal payload | No GC root; the storage has process/object lifetime and is never moved by the GC. |
| Managed runtime pointer / `Value` | Pointer or tagged runtime carrier owned by the sjulia runtime | Must be rooted across every safepoint unless it is an owned handle whose ownership contract keeps it live. |

Current partial Cranelift features are mapped through this table:

- local scalar code uses native scalar values;
- local tuples, scalar-field user structs, and `Complex` use native stack
  aggregates;
- local `String` literals use static data pointers;
- arrays, heap strings, heap structs, `Any`, multi-variant `Union`, exceptions,
  and runtime `Value` carriers remain managed runtime values and stay gated.

## Function ABI

Functions that can allocate, poll a safepoint, or manipulate managed runtime
values must use an explicit runtime context parameter:

```text
ctx: *mut SjuliaGcContext
```

The context parameter is a hidden first parameter for internally-generated
Cranelift functions once managed values are enabled. C ABI exported wrappers are
responsible for receiving or constructing the context before calling the
internal function. Scalar-only functions may keep the current context-free ABI.

Managed value returns are handles/pointers owned by the runtime context. Returning
a native scalar or native stack aggregate remains allowed only when the ABI can
represent the value without runtime ownership.

## Varargs and Keyword Call Adapters

Issue #7118 owns the Cranelift adapter contract for calls that use varargs,
argument splats, keyword arguments, or keyword splats. The adapter boundary is
above the low-level Cranelift call instruction: generated Cranelift functions
continue to call fixed signatures, while adapter lowering normalizes Julia call
syntax into that fixed signature.

Direct fixed-arity calls remain the fast path. An adapter is required when any
of the following is present:

- a callee method with a varargs parameter;
- a positional argument splat (`args...`);
- keyword arguments (`f(; x=...)`) or keyword defaults;
- a keyword splat (`kwargs...`);
- a call that needs runtime arity/type/keyword validation before selecting the
  fixed native target.

### Positional Varargs

Static splats are expanded before Cranelift lowering when the splatted operand
has a statically-known tuple shape and every expanded field has a native carrier.
This produces the same low-level `Call` / `CallMulti` shape as a hand-written
fixed-arity call and does not allocate a heap tuple.

True varargs tails use a Julia tuple carrier. A tail with statically-known native
field types may lower as a native stack tuple when it never crosses a runtime
boundary. Otherwise the adapter boxes the tail through the runtime tuple helper:

```text
__sjulia_tuple_pack(ctx, argc, values_ptr, type_tags_ptr) -> *mut SjuliaValue
```

`values_ptr` points to pointer-sized slots containing either native scalar
payloads widened to the helper carrier or managed handles. `type_tags_ptr`
describes the corresponding Julia element type tags. Tuple packing allocates and
is a safepoint; every managed argument must be rooted before the helper call.

### Keyword Canonicalization

Keyword calls lower through a keyword adapter symbol rather than by changing the
callee's native body signature. The adapter:

1. canonicalizes keyword names to the callee keyword signature order;
2. detects duplicate and unexpected keywords;
3. evaluates missing defaults in the adapter's Julia-defined order;
4. routes the normalized positional and keyword values to the fixed native
   target or to runtime dispatch when the target cannot be selected statically.

Generated symbol names use a stable adapter key:

```text
__sjulia_kw_adapter_<method_id>_<positional_arity>_<kw_name_hash>
```

The key is an implementation detail, but it must be deterministic for object
output so repeated builds produce the same imports/definitions.

### Keyword Splats

A keyword splat with a statically-known `NamedTuple` shape is expanded like
source keywords, then canonicalized by the adapter. Dynamic keyword splats use a
runtime NamedTuple carrier:

```text
__sjulia_namedtuple_pack(ctx, kw_count, name_ids_ptr, values_ptr, type_tags_ptr)
  -> *mut SjuliaValue
```

Dynamic keyword iteration, duplicate-name detection, and conversion to the
callee keyword order may throw, so the adapter follows the `SjuliaCallStatus`
exception model from Issue #7108.

### Adapter Gate Rule

A varargs/kwargs call site may lower to Cranelift only when one of these is
true:

- all splats and keywords are statically expanded to a fixed native signature;
- an adapter function is generated with a fully representable native signature;
- the runtime tuple/NamedTuple packing helpers and exception propagation path
  are available for the managed boundary.

Otherwise the backend must keep the existing explicit diagnostic instead of
guessing an arity, keyword order, or boxed tuple representation.

## Allocation Hooks

Heap allocation is routed through imported runtime functions owned by Issue
#7105. The contract is:

```text
__sjulia_gc_alloc(ctx, size, align, type_tag) -> *mut u8
__sjulia_array_alloc(ctx, elem_tag, len, ndims, dims_ptr) -> *mut SjuliaArray
__sjulia_string_alloc(ctx, byte_len) -> *mut SjuliaString
```

Every allocation hook is an allocating safepoint. Any live managed pointer not
owned by the returned value must be rooted before the call.

### Hook ABI Details

All hook parameters use fixed-width C ABI carriers:

| Parameter | Carrier | Meaning |
|---|---|---|
| `ctx` | pointer-sized `*mut SjuliaGcContext` | Runtime/GC context. Must be non-null for every managed allocation. |
| `size` | `u64` | Requested payload byte count for raw allocations. |
| `align` | `u32` | Minimum alignment in bytes. Must be a power of two. |
| `type_tag` | `u32` | Runtime-managed object kind tag. Tag assignment is runtime-owned; Cranelift treats it as an opaque constant. |
| `elem_tag` | `u32` | Runtime array element tag matching the VM `ArrayElementType` projection. |
| `len` | `u64` | Linear element count. Multidimensional shape product must match this count. |
| `ndims` | `u32` | Number of dimensions. `1` is a Vector; `0` is a scalar/zero-rank array only when Julia semantics explicitly require it. |
| `dims_ptr` | `*const u64` | Pointer to `ndims` dimension lengths in row-major metadata order. May be null only when `ndims == 0`. |
| `byte_len` | `u64` | String byte length excluding any trailing NUL. |

Return values are null on allocation failure. Cranelift generated code must not
continue with a null managed pointer: the failure path is a runtime exception
transition owned by Issue #7108. Until exception lowering exists, allocation
call sites remain gated instead of emitting an unchecked null path.

The imported symbols use the platform C calling convention selected for the
Cranelift module. Object output declares them as unresolved imports; JIT output
must bind them through the runtime symbol resolver before any managed allocation
path is enabled.

### Hook Selection

`__sjulia_gc_alloc` is the untyped primitive allocator for future heap structs
and runtime `Value` payloads. Typed front-end hooks should be preferred when the
runtime object shape has extra invariants:

- arrays use `__sjulia_array_alloc` so shape and element metadata are initialized
  atomically with storage;
- heap strings use `__sjulia_string_alloc` so byte length and UTF-8 payload
  ownership are initialized under the string type tag;
- future exception objects and boxed `Any` values may add typed hooks rather
  than exposing partially-initialized raw memory to generated code.

All hooks are safepoints even when a concrete implementation uses a bump
allocator. This keeps generated code independent from the collector strategy.

## Root Set

Cranelift lowering must materialize rootable managed values in pointer-sized
stack slots before a safepoint. The runtime sees roots through:

```text
__sjulia_gc_root_push(ctx, slots_ptr, slot_count)
__sjulia_gc_root_pop(ctx, slot_count)
```

Roots are lexical in generated code. A root push must dominate every safepoint
that needs the value, and the matching pop must post-dominate the last use in
the protected region. Native scalar and native stack aggregate values are not
root slots unless they contain managed fields.

## Safepoints

The following operations are safepoints:

- allocation hooks;
- calls to runtime dispatch or helpers with unknown allocation behavior;
- explicit loop/backedge polls once managed values are enabled;
- exception throw/catch/unwind transitions.

Scalar libm calls and pure Cranelift arithmetic are non-safepoints. A helper
with unknown effect is treated as a safepoint until classified otherwise.

## Stack Maps

Precise GC requires stack map metadata at safepoints. Issue #7106 owns the
Cranelift stack-map integration. Until stack maps or the runtime root-stack
fallback described below are emitted, managed heap values must remain rejected
even if allocation/root helper imports are present.

The stack map must identify:

- root stack slots registered with the runtime;
- managed pointer SSA values spilled by Cranelift at a safepoint;
- the safepoint ID passed to the runtime poll/allocation path.

### Safepoint IDs

Every generated safepoint has a stable numeric ID scoped to the function:

```text
SafepointId {
  function_symbol: string,
  ordinal: u32,
}
```

The ordinal is assigned in codegen order after low-level IR lowering and before
Cranelift function finalization. Reordering within Cranelift is handled by the
emitted stack map or by the explicit root-stack fallback; high-level AoT
statement indexes are not used as runtime safepoint IDs.

### Root Slot Descriptors

For each safepoint, generated metadata records the live managed slots:

```text
RootSlot {
  slot_index: u32,
  offset_from_frame_base: i32,
  kind: managed-pointer | tagged-value,
}
```

`slot_index` is the order passed to `__sjulia_gc_root_push`. The runtime can use
the explicit root stack directly; `offset_from_frame_base` is the precise stack
map location used by a future collector that scans native frames without relying
only on the root stack.

### Cranelift Integration Strategy

The preferred implementation is:

1. materialize every live managed value in a pointer-sized Cranelift stack slot;
2. register those slots with `__sjulia_gc_root_push`;
3. attach a stack map or equivalent side metadata at each allocation/poll call;
4. pass the safepoint ID to allocation/poll helpers so the runtime can find the
   matching root metadata;
5. pop roots after the protected region.

If Cranelift 0.115 does not expose a usable stack map emission API for the
selected JIT/object path, the explicit root stack is the compatibility fallback:
managed values may be enabled only when every live managed pointer at a
safepoint is present in pushed root slots. Native-frame precise scanning then
remains disabled, but GC safety does not depend on discovering arbitrary
Cranelift spills.

### Gate Rule

A managed-value Cranelift lowering site is allowed only if it can prove one of:

- a precise Cranelift stack map is emitted for the safepoint; or
- all live managed values are present in explicit root slots pushed before the
  safepoint and popped after the final use.

Otherwise the site must continue to emit the existing unsupported diagnostic
instead of generating unchecked heap code.

## Ownership

Issue #7107 owns the non-Copy ownership model. The contract chosen here is
GC-managed ownership for strings, arrays, heap structs, and runtime `Value`
objects. Cranelift generated code must not free these values directly and must
not use ARC-style retain/release unless a later issue explicitly changes this
contract.

Read-only string literal payloads are excluded from GC ownership because they
live in immutable object/JIT data sections.

### Managed Handles

Managed heap values use pointer-sized handles. Copying a handle copies the
reference, not the object. A copied handle is safe across a safepoint only when
the underlying object is reachable from one of:

- an explicit root slot registered with the current `SjuliaGcContext`;
- another rooted/owned managed object field;
- static read-only data that is outside GC ownership.

Generated Cranelift code must not duplicate ownership by cloning runtime storage
or by freeing a handle. Lifetime is controlled by reachability from roots.

### String Ownership

There are two string classes:

| Class | Owner | Mutability | Rooting |
|---|---|---|---|
| Read-only literal payload | Object/JIT data section | Immutable | No GC root. |
| Heap string | GC runtime | Immutable after construction | Managed handle; root across safepoints. |

`String` concatenation, substring materialization, parsing, and any operation
that allocates a new string must return a heap string handle. Local literal
payloads may be passed to non-allocating length/read-only helpers, but must be
boxed/copied into a heap string before crossing a runtime `Value`, array, or
mutable ownership boundary.

### Array Ownership

Array handles own shape metadata and a GC-managed data buffer. The data buffer is
not copied when the handle is copied. Mutating operations require a rooted array
handle for the whole mutation window because allocation or bounds-error paths can
be safepoints.

The runtime owns:

- element type tag and rank;
- dimensions and linear length;
- data buffer capacity and initialized length;
- references from array elements to other managed values.

Generated Cranelift code may load/store scalar elements directly only after the
array handle and buffer pointer are rooted and the bounds/shape metadata has
been checked for the access. Arrays with managed element types require write
barrier support before mutation can be enabled.

## Array / Vector Heap Layout and Lowering

Issue #7098 owns the Cranelift Array/Vector lowering contract. Arrays are
GC-managed `SjuliaArray*` handles with a runtime-owned header and data buffer.
`Vector{T}` is `Array{T,1}` with `ndims == 1`; `Matrix{T}` is `Array{T,2}`.

Cranelift must not mirror the Rust `ArrayValue` struct layout. It may generate
memory operations only against the stable runtime ABI fields below, whose byte
offsets are emitted as codegen constants by the runtime ABI table:

```text
type SjuliaArray = opaque runtime object

SjuliaArrayHeader {
  type_tag: u32,
  elem_tag: u32,
  ndims: u32,
  flags: u32,
  len: u64,
  capacity: u64,
  dims_ptr: *const u64,
  data_ptr: *mut u8,
}
```

`len` is the linear element count. `capacity` is the number of element slots in
`data_ptr`, not bytes. `dims_ptr[d]` stores the length of Julia dimension
`d + 1` in source order. The product of all dimensions must equal `len`.
`data_ptr` is aligned for the concrete element carrier.

### Element Carriers

The initial direct-memory lowering is limited to native scalar element carriers:

| Element type | Data representation |
|---|---|
| `Bool` / integer / floating / `Char` | Packed native carrier with the same width as the Cranelift scalar type. |
| `Nothing` | Zero-size logical element; direct load/store is not emitted. |
| heap string / array / struct / `Any` / multi-variant `Union` | Pointer-sized managed handle or `SjuliaValue*`; mutation remains gated until write barriers are implemented. |

For native scalar elements, `getindex` emits a typed load from
`data_ptr + zero_based_linear_index * sizeof(T)`. `setindex!` emits a typed
store to the same address after the array handle and buffer pointer have been
rooted through any possible safepoint.

### Allocation

Array allocation lowers through the typed allocation hook from Issue #7105:

```text
__sjulia_array_alloc(ctx, elem_tag, len, ndims, dims_ptr) -> *mut SjuliaArray
```

Generated code must compute `len` with checked multiplication of the dimensions.
Overflow and null allocation results branch to the status-based exception path
from Issue #7108. The dimensions passed at `dims_ptr` may live in a temporary
native stack slot for the duration of the allocation call because the runtime
copies shape metadata into the returned `SjuliaArray`.

### `length` and `size`

`length(A)` is a non-allocating header load of `len` and returns native `Int64`
after the array handle has been proven non-null. `size(A, d)` checks
`1 <= d <= ndims` and loads `dims_ptr[d - 1]` as native `Int64`; invalid
dimensions branch to the #7108 exception path.

`size(A)` may lower to a native stack tuple when rank is statically known and
all tuple fields are native `Int64`. Unknown-rank or runtime `Value` tuple
returns remain gated until the runtime value boundary can allocate/box the
result.

### Indexing and Bounds

Cranelift indexing preserves Julia's 1-based semantics.

For linear indexing:

```text
if index < 1 || index > len:
  __sjulia_bounds_error(ctx, A, index, 1)
zero_based = index - 1
```

For Cartesian indexing with `ndims == N`, each source index `i_d` is checked
against `1 <= i_d <= dims[d - 1]`. The zero-based linear index is column-major:

```text
stride_1 = 1
stride_d = product(dims[1], ..., dims[d - 1])
linear = sum((i_d - 1) * stride_d for d in 1:N)
```

All multiply/add operations used for byte offsets are overflow-checked before
forming the final address. Bounds or overflow failure calls the exception helper
and follows the `SjuliaCallStatus` propagation rules.

`@inbounds` may remove the user-facing bounds branch only when the lowering
already proved the array handle non-null, element carrier, rank, and byte offset
range. It must not suppress rooting, write barriers, or allocation failure
checks.

### Borrowing

Temporary borrowed pointers into string bytes or array buffers may be introduced
only inside a region with no safepoint. If a helper call can allocate, throw, or
call runtime dispatch, borrowed pointers must not be live across that call; keep
the owning managed handle rooted and reacquire the borrow after the safepoint.

## Runtime Value Boundary

Issue #7102 owns the `Any` and multi-variant `Union` boundary. Cranelift uses an
opaque pointer-sized handle for runtime values:

```text
type SjuliaValue = opaque runtime object
carrier: *mut SjuliaValue
```

`SjuliaValue` is GC-managed and has a runtime-owned tag/header. Cranelift does
not inspect the object layout directly; it calls runtime helpers for tag checks,
boxing, unboxing, dispatch, and display. This keeps the native backend from
duplicating the VM `Value` enum layout or relying on Rust-specific enum ABI.

### Representation Rules

| Julia/AoT type | Cranelift boundary carrier |
|---|---|
| `Any` | `*mut SjuliaValue` |
| multi-variant `Union` | `*mut SjuliaValue` plus runtime tag check before typed use |
| single-variant `Union{T}` | Same carrier as `T` when `T` is native-representable |
| `Union{}` | unreachable/bottom; no runtime carrier |
| runtime boxed scalar | `*mut SjuliaValue` |
| heap string / array / struct crossing `Any` | boxed `*mut SjuliaValue` that owns or references the managed object |

Native scalar code should avoid boxing unless a value crosses an `Any`,
multi-variant `Union`, dynamic dispatch, exception, or C ABI runtime boundary.

### Runtime Value Helpers

The minimum helper ABI is:

```text
__sjulia_value_type_tag(ctx, value) -> u32
__sjulia_value_box_i64(ctx, value) -> *mut SjuliaValue
__sjulia_value_box_f64(ctx, value) -> *mut SjuliaValue
__sjulia_value_box_bool(ctx, value) -> *mut SjuliaValue
__sjulia_value_box_ptr(ctx, type_tag, ptr) -> *mut SjuliaValue
__sjulia_value_unbox_i64_checked(ctx, value, expected_tag) -> i64
__sjulia_value_unbox_f64_checked(ctx, value, expected_tag) -> f64
__sjulia_value_unbox_bool_checked(ctx, value, expected_tag) -> u8
```

Boxing helpers allocate and are safepoints. Checked unboxing may throw a
runtime type error and is therefore an exception boundary; until #7108 lands,
Cranelift must keep checked unboxing sites gated unless the type was already
proven by static lowering and no runtime failure path is possible.

### Rooting Rules

Every `*mut SjuliaValue` is a managed pointer. It must be rooted across:

- allocation hooks;
- boxing helper calls;
- dynamic dispatch;
- checked unboxing or type assertion helpers;
- exception transitions;
- loop/backedge safepoint polls once managed values are enabled.

`RuntimeOwned` in the existing AoT rooting verifier means the Rust backend owns
a safe Rust `Value`. In Cranelift, the corresponding value is a rooted or
otherwise reachable `SjuliaValue*`; generated code must not treat an unrooted raw
pointer as satisfying the safepoint contract.

### Union Narrowing

For a multi-variant `Union`, Cranelift must either:

1. keep the value boxed as `SjuliaValue*` through the whole region; or
2. emit a runtime tag check, branch by tag, unbox in each branch, and re-box at
   control-flow joins that need a union value.

Phi nodes joining different concrete carriers must use boxed `SjuliaValue*`
unless all incoming values share one native carrier and the union has been
statically narrowed to that carrier.

## Exception and Unwinding Model

Issue #7108 owns the Cranelift exception model. Cranelift generated code must
not use native unwinding across generated frames, Rust frames, or C ABI export
wrappers. Exceptions are runtime-managed `SjuliaValue*` handles recorded in the
`SjuliaGcContext`, and propagation is explicit control flow in generated code.

This chooses a Result-style ABI over landing pads or `setjmp`/`longjmp`:

```text
SjuliaCallStatus = u32
SJULIA_CALL_OK = 0
SJULIA_CALL_EXCEPTION = 1
```

Scalar-only functions that cannot allocate, throw, or call throwing helpers may
keep the current direct scalar ABI. Any function that can throw uses the hidden
`ctx: *mut SjuliaGcContext` parameter and returns a status result in addition to
its normal value results. In Cranelift multi-result form, the status is result
0 and user values are trailing results; for an ABI that cannot carry multiple
results, user values are lowered through out-pointers owned by the wrapper.

### Exception Helper ABI

The minimum helper surface is:

```text
__sjulia_throw_value(ctx, value) -> SjuliaCallStatus
__sjulia_exception_is_pending(ctx) -> u8
__sjulia_exception_peek(ctx) -> *mut SjuliaValue
__sjulia_exception_take(ctx) -> *mut SjuliaValue
__sjulia_exception_clear(ctx)
__sjulia_bounds_error(ctx, owner, index, axis) -> SjuliaCallStatus
__sjulia_type_error(ctx, value, expected_tag) -> SjuliaCallStatus
```

`__sjulia_throw_value` records a rooted exception object in `ctx` and returns
`SJULIA_CALL_EXCEPTION`; it does not unwind the native stack. Error construction
helpers such as bounds and type errors may allocate, so they are safepoints and
can fail only by leaving a pending exception in `ctx`.

### Propagation Rules

Generated code checks the status result after any call that can throw. On
`SJULIA_CALL_OK`, it consumes the trailing user values. On
`SJULIA_CALL_EXCEPTION`, it must not read uninitialized user value results; it
branches to the nearest active catch/finally edge or to the function epilogue
that returns exception status to its caller.

Local throw sites lower to:

1. root the thrown `SjuliaValue*`;
2. call `__sjulia_throw_value`;
3. branch to the current exception edge.

Allocation failure, checked unboxing failure, bounds errors, dynamic dispatch
failure, and future write-barrier failure use the same status propagation path.

### `try` / `catch` / `finally`

`try` / `catch` lowers to explicit basic blocks:

- protected body normal exit branches to the post-try block;
- throwing calls inside the protected body branch to the catch block on
  `SJULIA_CALL_EXCEPTION`;
- the catch block obtains the thrown value with `__sjulia_exception_take(ctx)`,
  binds the Julia catch variable, clears the pending state, and executes the
  catch body;
- if no catch handles the exception, the exception status is re-returned to the
  caller after any required `finally` body.

`finally` lowers as a cleanup block that post-dominates both normal and
exceptional exits from the protected body. If the cleanup itself throws, its
new pending exception replaces the previously pending one, matching Julia's
"later throw wins" behavior for cleanup failure.

### Export Boundary

C ABI exported wrappers never let an sjulia exception escape as native unwind.
They inspect the returned status and the pending exception in `ctx`, then map it
to the existing exported error carrier for that embedding surface. Object and
JIT paths use the same helper ABI so exception behavior does not depend on the
artifact mode.

## Gate Policy

The allocation hook ABI is defined, but Cranelift must keep rejecting managed
runtime values until runtime helper implementations/bindings, array/string heap
lowering (#7098), and exception paths (#7108) are in place. This keeps #7098
arrays, heap strings, runtime `Value`/`Any`, and exception objects out of the
backend until their root and safepoint obligations are representable.
