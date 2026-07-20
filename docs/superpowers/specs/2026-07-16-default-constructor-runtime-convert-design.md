# Default Constructor Runtime Convert Design

Date: 2026-07-16
Issues: #11147, #10593, #11346, #11347, #11348, #11349, #11350
Related: #10813, #11146, #8102, #8103, #7793, #10502

## Goal

Make sjulia's synthesized default struct constructors follow Julia's ordinary
method and conversion semantics. Direct syntax and first-class `DataType`
calls must select the same constructor, successful construction must dispatch
visible `convert` methods, and conversion failures must reach the surrounding
`try`/`catch` with the upstream exception class. No path may allocate a value
whose runtime field value violates its declared field type.

## Problem

sjulia currently models a default constructor as two unrelated shortcuts.
Static calls reach `compile_struct_constructor`, which applies
`compile_expr_as` before emitting allocation bytecode. Runtime calls through a
bound `DataType` use best-effort VM helpers that convert only selected native
numeric values and discard conversion errors. These shortcuts explain the
entire #11147 constructor sweep:

- a known unsupported value aborts compilation instead of raising a catchable
  `MethodError` (#10593);
- an inexact numeric conversion truncates or leaves the source value in place
  instead of raising `InexactError` (#11346 and #11347);
- a user-defined `Base.convert(::Type{FieldType}, value)` is never called
  (#11348);
- bare parametric construction converts arguments that should first have to
  match the synthesized field-typed outer method (#11349);
- the native float converter parses numeric strings, conflating `convert` with
  `parse` (#11350).

The static and runtime failures are not separate bugs. Both are consequences
of omitting the constructor methods that upstream Julia synthesizes.

## Upstream Model

`julia/src/method.c::jl_ctor_def` defines the exact-field outer first when a
struct has no user-declared inner constructor:

1. An ordinary outer constructor whose value signature is the exact declared
   field-type vector. Every non-parametric struct gets this outer. A parametric
   struct gets it only when every type parameter can be inferred from the field
   types. It does not convert arguments to make its signature applicable. The
   non-parametric outer executes `new` directly; the parametric outer delegates
   a matching call to the explicit `Type{Foo{T...}}` default inner constructor.
2. Unless the struct is non-parametric and every field is `Any`, it also defines
   a default inner constructor whose value arguments are `Any`. For every
   non-`Any` field the inner evaluates the target type once, preserves the value
   when `value isa target`, otherwise calls ordinary `convert(target, value)`,
   and only then executes `new`. The all-`Any` condition is vacuously true for a
   zero-field non-parametric struct, so that case also omits the inner.

For example, `struct Plain; x::Int64; end` produces both
`Tuple{Type{Plain},Any}`, the converting BareInner row, and
`Tuple{Type{Plain},Int64}`, an ordinary direct-`new` outer row. Declared `Any`
fields remain `Any` in the outer signature. An all-`Any` or zero-field
non-parametric struct instead exposes exactly one ordinary direct-`new` outer
row: the identical inner is never defined, so the row has no
`ConstructorSelfFamily` origin and there is no generated collision to resolve.

Any user-declared inner constructor suppresses both defaults. Conversion is
therefore normal Julia dispatch in a normal method body, not allocator policy.

## Selected Design

### Compiler-owned synthetic method registry

Generate default constructor methods after lowering, while preserving
`StructDef.inner_constructors` as the authoritative list of constructors the
user actually declared. Do not append synthetic methods to that vector: many
compiler and VM guards intentionally interpret a nonempty vector as “default
constructors are suppressed.”

Register synthetic methods through the same method tables, inference engine,
`FunctionInfo` list, bytecode compiler, cache serialization, and runtime
callable-candidate machinery as source-written methods.

- A non-parametric struct with at least one non-`Any` field gets a bare-self
  default inner method with one `Any` parameter per field.
- A parametric struct gets an explicit-self `Foo{T...}` default inner method
  with the struct's type parameters and `Any` value parameters.
- A non-parametric struct also gets an ordinary bare `Foo(...)` outer method
  whose signature is the exact declared field-type vector and whose body
  executes `new` directly.
- For an all-`Any` or zero-field non-parametric struct, register only that
  ordinary outer. Do not register an identical inner row or attach
  `ConstructorSelfFamily` metadata to the outer.
- A parametric struct gets an ordinary bare `Foo(...)` outer method only when
  every declared type parameter is structurally inferable from the field type
  expressions. Its signature is the exact declared field-type vector and its
  body delegates to `Foo{T...}(...)`.

Synthetic inner rows use the existing serialized `ConstructorSelfFamily`
carrier. Synthetic outer rows are ordinary methods. Source definition order
remains authoritative: declared `Any` fields stay `Any` in the outer signature,
the upstream all-`Any`/zero-field early omission prevents an identical generated
inner, and a later user outer with the same canonical signature replaces the
default outer while distinct overloads coexist.

### Synthetic inner conversion body

For each non-`Any` field in the default inner, synthesize the equivalent of:

```julia
let target = <declared field type>, value = argument
    value isa target ? value : convert(target, value)
end
```

The target and value are each evaluated once. Type-variable and nested
parametric field expressions are materialized structurally from the active
constructor bindings; module-private nominal types resolve in the struct's
defining module. Converted values are evaluated left-to-right and passed to a
single `new` only after all conversions succeed, so allocation is
transactional while conversion side effects retain Julia order.

This body uses the existing `Convert` builtin, which checks visible Julia
`convert` methods before its native fallback. It therefore supports custom
field conversions without a re-entrant VM call loop or a constructor-specific
dispatch implementation.

### Constructor selection

Constructor syntax must see the synthetic rows before legacy raw allocation:

- explicit `Foo{T...}(args...)` selects the synthetic explicit-inner row and
  binds the callable self parameters;
- bare `Foo(args...)` selects the exact-field typed ordinary outer when its
  signature applies, for either a non-parametric struct or an inferable
  parametric struct;
- a non-parametric call that does not match that typed outer falls back to the
  synthetic `Any`-argument bare-inner row and its ordinary conversion body,
  even though `StructDef.inner_constructors` remains empty;
- an all-`Any` or zero-field non-parametric call selects its sole ordinary
  direct-`new` outer; no inner-origin candidate participates;
- first-class `DataType` calls, splatted calls, and higher-order calls reuse the
  same runtime method candidates and therefore execute the same bodies.

The legacy field-count allocator remains only a compatibility fallback for
paths without a method candidate. It must become fail-closed: fallible native
coercion propagates `MethodError` or `InexactError`, and an unresolved mismatch
must never be stored. It is defense-in-depth, not the authority for custom
`convert` dispatch.

### Native convert correction

Remove String parsing from the native Float16/Float32/Float64 `convert`
fallback. Numeric parsing belongs to `parse`; `convert(Float64, ::String)` has
no upstream method and must raise `MethodError`, even when the text is
numeric-looking. Constructor conversions automatically inherit this result.

### Cache and incremental compilation

Synthetic methods are compiled into the ordinary function/method universe and
must survive Base-cache serialization, restoration, method-table cloning, and
REPL delta compilation. Bump the Base cache version/fingerprint because fresh
compilation now contains additional constructor methods and bytecode. A cache
restore must not regenerate duplicate synthetic rows; user structs compiled in
a later delta must still receive their own defaults.

## Considered Alternatives

### Emit a runtime `ThrowMethodError` for the #10593 shape

Rejected. A value may have a valid user-defined conversion, or conversion may
raise `InexactError`, `TypeError`, or a user exception. A preselected
`MethodError` would encode the observed example rather than Julia semantics.

### Make the existing VM coercion helpers fallible and stop there

Rejected as the semantic fix. This prevents corrupt allocation for native
numeric fields but cannot call a user-defined `convert` without introducing a
new continuation state into the VM instruction loop. It also leaves direct and
bound constructor selection on different authorities. Fallible helpers remain
useful only as a safety net.

### Convert directly at every static constructor call site

Rejected. It cannot cover `g = Foo{T}; g(x)`, `map(g, xs)`, runtime type
arguments, cache-restored callables, or reflection consistently. It would also
continue to bypass normal method specificity.

### Mutate `StructDef.inner_constructors`

Rejected. The vector records user syntax and is used repo-wide to suppress the
default constructor. Appending a default to an empty vector would make the
compiler believe the default itself suppresses the default and would alter
cache/REPL guards that distinguish explicit user policy.

## Test Strategy

Follow red-green-refactor and verify every fixture against Julia first.

1. Add separate static and runtime exception fixtures so a compile abort in
   one lane cannot mask bound-call corruption in another.
2. Cover unsupported conversion (`MethodError`), inexact conversion
   (`InexactError`), exact numeric success, numeric-looking String rejection,
   custom struct conversion, multi-field late failure, and no-allocation after
   failure.
3. Exercise direct, bound, runtime type-argument, splatted, and `map` calls for
   parametric and non-parametric structs.
4. Cover exact-field typed ordinary outers for both struct families: a matching
   non-parametric call takes the direct-`new` outer, a non-parametric mismatch
   falls back to the converting BareInner, and a bare-parametric mismatch
   rejects with `MethodError` while the explicit `{T}` inner continues to
   permit conversion.
5. Add compiler/lowering tests for both generated rows on a non-parametric
   typed-field struct, mixed `Any` positions, and the all-`Any`/zero-field
   early-omit cases. Assert that each early-omit case has exactly one ordinary
   direct-`new` outer row with no `ConstructorSelfFamily` origin and the same
   global index/body after cache restoration. Also cover default suppression,
   parametric inferability, module qualification, and method replacement.
6. Replace VM tests that currently encode silent coercion/corrupt storage with
   fail-closed expectations.
7. Keep #7793, #8102, #8103, #8121, #9829, and #10502 as regression gates.
8. Expand the exception parity probe and documentation so every raise-layer
   constructor row is a permanent sentinel.

## Error Behavior

- No applicable bare-parametric default outer method: catchable `MethodError`.
- A non-parametric mismatch skips the typed outer and reaches the converting
  BareInner.
- Applicable constructor but no field conversion method: catchable
  `MethodError` from ordinary `convert`.
- Applicable conversion with an unrepresentable value: catchable
  `InexactError`.
- User conversion throws: propagate that exact exception.
- Any failure occurs before `new`, so the heap receives no partial instance.

## Success Criteria

- Direct and first-class constructor forms agree for all sweep cases.
- User-defined field conversion runs through ordinary dispatch.
- Exact-field outer matching remains distinct from non-parametric BareInner
  fallback and explicit-parametric inner conversion.
- All-`Any` and zero-field non-parametric structs expose exactly one ordinary
  direct-`new` outer, fresh and cache-restored.
- No constructor fallback can retain a mismatched value after conversion
  failure.
- Fresh and cache-restored execution produce identical results.
- The #11147 raise-layer sweep has no remaining constructor divergence and
  closes #10593, #11346, #11347, #11348, #11349, and #11350.
- Full release tests, clippy lanes, formatting, cache audits, and the guarded
  merge gate pass before a regular merge.
