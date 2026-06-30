# AoT Call / Control-Flow Contracts

**Status**: Milestone 29 contract surface.
**Scope**: Issues #7032, #7043, #7047, #7053, #7054, #7055.

This document fixes the AoT lowering boundary for Julia features whose syntax is
already visible in Core IR but whose full behavior requires runtime call adapters,
closure environments, broadcast allocation, or exception status propagation.

## Exceptions: `try` / `catch` / `finally` (Issue #7032)

AoT does not use Rust unwinding as the primary Julia exception carrier. Exception
paths must lower to an explicit status-bearing boundary:

- fallible runtime calls return a `SjuliaCallStatus`-style result or an equivalent
  generated Rust `Result<T, SjuliaException>`;
- `catch e` receives the Julia exception object, not a Rust panic payload;
- `finally` runs on both success and error paths before the status is returned or
  rethrown;
- `rethrow` preserves the current exception object and stack context where the
  runtime supplies it.

Until Core IR -> AoT IR can preserve catch variables, finally execution ordering,
and rethrow state, `try` / `catch` / `finally` remains a span diagnostic gate.
Generated Rust must not silently compile only the success path.

## Varargs and Splatting (Issue #7043)

AoT distinguishes static splats from runtime varargs:

- fixed tuple splat at a known call site is expanded into positional arguments;
- fixed-count `Vararg{T,N}` can lower to a fixed native signature when each element
  type is static;
- open `args...` collects the tail into a runtime tuple/array handle when the
  arity is not statically closed;
- dynamic `f(xs...)` uses the runtime call adapter rather than cloning ad hoc Rust
  vectors into a guessed signature.

`--pure-rust` accepts only the fixed native forms. Open varargs and dynamic splat
sites stay gated until tuple packing and adapter dispatch helpers are connected.

## Broadcast and Fusion (Issue #7047)

Broadcast lowering uses one shape plan per fused broadcast expression:

- scalar operands are expanded without allocating;
- arrays must have compatible axes before the element loop is emitted;
- nested dot calls such as `a .+ b .* c` lower as a single fused expression tree,
  not as materialized temporaries;
- dynamic rank, unknown axes, and element types requiring runtime `Value`
  dispatch use the runtime broadcast helper boundary.

AoT may generate direct Rust loops for static 1D/2D scalar element broadcasts.
Fusion that cannot prove shape and element types remains a diagnostic gate until
the runtime broadcast helper can carry axes, allocation, and exception status.

## First-Class Functions (Issue #7053)

Function values use two carriers:

- known monomorphic callees become generated function items or `fn` pointers with
  a fixed AoT signature;
- unknown callees, method tables, closures, or values returned from functions use
  a runtime callable handle.

Passing or returning a function is allowed only when the carrier is known. A
runtime callable handle is required for higher-order dispatch that cannot be
resolved statically; `--pure-rust` gates those sites.

## Do Blocks (Issue #7054)

`do` block support follows Julia lowering: the block is an anonymous function
argument to the surrounding call. AoT therefore treats `do` blocks as closure
arguments after lowering, with no separate codegen shortcut.

Non-capturing do blocks may use the first-class known-callee path. Capturing do
blocks require the closure environment contract below. If the callee requires a
runtime callable, the site follows the first-class function gate.

## Closures and Lambdas (Issue #7055)

Closure lowering uses an explicit environment:

- immutable captures may be copied into the environment by value;
- mutated or shared captures require a by-reference cell so Julia mutation
  semantics are preserved;
- non-capturing lambdas lower like generated static functions;
- capturing closures passed to higher-order functions use either a monomorphic
  generated environment struct or a runtime callable handle.

AoT must not erase captures or convert mutable captures into independent copies.
Until environment layout, call shims, and runtime callable handles are connected,
unsupported capture/dispatch shapes stay behind span diagnostics.

## Gate Rule

For all features in this document, unsupported sites must fail during `--check`,
`--pure-rust`, or codegen with a span-bearing diagnostic. Emitting Rust that
silently changes Julia dispatch, exception, capture, or broadcast semantics is
out of scope.
