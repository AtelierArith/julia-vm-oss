# Call Resolution

Tracks Issue #10461.

## Semantic Boundary

Every call path must resolve Julia semantics before choosing an optimized
executor. The shared diagnostic contract lives in
`subset_julia_vm_types::inference_core::dispatch_resolver`:

```text
CallRequest {
  callee, positional, keywords, lexical_scope, world, call_span, candidates
}
    -> ResolvedCall::{JuliaMethod, Intrinsic, Constructor, Dynamic, Error}
```

The request uses structured `CoreType` arguments and constructor identities,
complete owner paths for named functions, explicit keyword origin, lexical
module/method identity, runtime world, source span, and the exact candidate
set. A Julia method result carries a `MethodId` plus `TypeBindings`.
`TypeBindings::NotObserved` is an explicit migration state for legacy adapters;
it must not be treated as an empty binding environment.

`MethodId`, `IntrinsicId`, `IntrinsicContractId`, and `ConstructorTargetId` are
semantic boundary types, not callee spellings. The Phase 0 runtime adapter's
`MethodId` is VM-local and is not a cache serialization identity.

## Current Entry Points

| Call family | Lexical/candidate resolution | Shared selection | Execution |
| --- | --- | --- | --- |
| Direct syntax | `compile/expr/call::compile_call` resolves locals, imports, modules, constructors, and method tables | `MethodTable::dispatch_inner` uses `selection::select_method` | direct bytecode, typed dispatch, or runtime call |
| Stored function/callable value | `collect_runtime_callable_candidates` | `dispatch_function_variable_for_values` tries structured value scoring, then the legacy scorer on a miss; parametric constructors retain an explicit legacy bridge | `call_runtime_callable_value` builds the ordinary frame |
| HOF callback | callback state carries the original `Value` | `call_runtime_callable_value` uses the same callable-value resolver | ordinary frame or checked intrinsic fallback |
| VM runtime dispatch | instruction supplies candidate method indices | `find_best_method_index_from_candidates` uses `selection::select_method` | ordinary/specialized frame |
| Constructor | owner-exact compiler and runtime constructor lookup | constructor methods use the same method-table/runtime selectors | checked constructor executor or generic fallback |
| Runtime specializer | consumes a lexically resolved body/callee; local bindings win | does not own a separate Julia method scorer | execution-only instruction selection |

The shared selection driver owns dominance, ambiguity, and final-pick control
flow. Compile-time and runtime adapters still provide representation-specific
matching closures. Name-keyed compiler handlers and specializer arms are
execution intercepts, not permission to bypass lexical resolution or choose a
different Julia method.

## Comparison Mode

Set `SJULIA_CALL_RESOLVER_COMPARE=1` to compare the legacy callable-value
scorer with the production value-aware selection on the same request.
The adapter is at `dispatch_function_variable_for_values`, reached by stored
functions, callable structs, constructors with method candidates, and HOF
callbacks. It records the callee, structured positional types, lexical context,
world, source span, candidate set, selected target, and binding observation.

Only differences are written to stderr, prefixed with
`SJULIA_CALL_RESOLVER_COMPARE:`. Ordinary calls return the production
value-aware result; the legacy result is evaluated only for comparison or
after a structured miss. Parametric constructors retain the legacy result as
an explicit migration bridge because their callable `Type{...}` head is still
encoded outside the positional signature consumed by the value matcher.
Imposing a head-first ordering would be incorrect: Julia compares the complete
callable-plus-positional signature and can report cross-dimension ambiguity
(Issues #10461/#11610).

Representative corpus:

```bash
SJULIA_CALL_RESOLVER_COMPARE=1 target/release/sjulia \
  subset_julia_vm/tests/fixtures/dispatch/qualified_function_value_identity_10077.jl
SJULIA_CALL_RESOLVER_COMPARE=1 target/release/sjulia \
  subset_julia_vm/tests/fixtures/dispatch/qualified_base_builtin_function_value_identity_10284.jl
SJULIA_CALL_RESOLVER_COMPARE=1 target/release/sjulia \
  subset_julia_vm/tests/fixtures/dispatch/parametric_ctor_callable_parity_10502.jl
SJULIA_CALL_RESOLVER_COMPARE=1 target/release/sjulia \
  subset_julia_vm/tests/fixtures/dispatch/callspecialize_resolved_function_10457.jl
```

The three ordinary-function fixtures emit no comparison-prefixed lines. The
parametric-constructor fixture currently emits the documented bridge deltas
where the positional-only proposed scorer selects `Rational{T}` and production
retains the concrete constructor target. New non-constructor differences or a
change in those constructor targets require investigation; fixture success
alone is not enough, so inspect stderr for the prefix.

## Fast-Path Review Rules

A new call fast path must identify a semantic target, state an exact
precondition, retain a generic fallback, and cover direct/callable, shadowing,
module, evaluation-order, and exception parity where applicable.

Existing source audits enforce the two most error-prone name-bearing areas:

- `scripts/check_specializer_callee_guard.sh` rejects a runtime-specializer
  name-keyed arm before lexical local-callee resolution.
- `scripts/check_constructor_owner_resolution.sh` inventories constructor leaf
  projections and rejects new owner-erasing constructor lookup.
- `scripts/check_compile_expr_local_shadow_guard.sh` rejects unguarded
  compile-time bare-name value fast paths.
- `scripts/check_call_function_variable_value_dispatch_order.sh` requires both
  callable-value and direct `CallDynamic` runtime paths to build and resolve a
  structured `CallRequest`; compiler expression paths may emit dynamic calls
  only through the identity-preserving `emit_dynamic_call` hub.

## Migration State

Phase 0 provided the request/result vocabulary, inventory, and differential
adapter. Phase 1a makes ordinary runtime function-value, callable-struct, HOF
callback, qualified runtime, and splat opcodes use the same value-aware
selector. Parametric constructors enter through the same boundary but retain
the documented legacy bridge until the request carries the full callable
signature. `invoke` remains an explicit declared-signature mode, including a
literal declared `Any` (Issue #11609).
Phase 1b makes `CallDynamic` retain the compiler-resolved method-table identity
in boxed `DynamicCallOperands`; bare and qualified direct-call cache misses now
build the same `CallRequest` and invoke the same structured runtime scorer as
callable values. The opcode audit rejects private scorer calls, anonymous
dynamic-call emission, and any runtime path that ignores the carried identity.
Remaining #10461 work is to return the complete `ResolvedCall` target/bindings,
pass keyword defaults through the request, register intercepts by resolved
target ID, and remove or mark every residual duplicate scorer as execution-only.
