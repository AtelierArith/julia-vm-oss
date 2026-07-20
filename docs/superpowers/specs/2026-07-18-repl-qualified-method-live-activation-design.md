# REPL Parametric and Keyword Method Live Activation Design

**Issue:** #9784  
**Date:** 2026-07-18  
**Status:** Approved for implementation

## Objective

Remove the remaining fresh full-recompile fallback that is selected solely
because a top-level Julia method has `where` parameters or keyword parameters.
Brand-new definitions, extensions, and same-signature replacements for these
methods must compile against the retained REPL compiler snapshot, install on the
held VM, publish in source order, refresh affected callers, and recover the
exact reached prefix after a Julia-catchable error.

This is the next LV3b retirement slice of #9784. It does not claim that the
separate type-, module-, import-, macro-, or opaque-`eval` fallbacks are retired.

## Upstream Shape

Upstream Julia publishes a `where` method as one method-table entry with its
`TypeVar` environment and assigns that entry a `primary_world`. Keyword syntax
lowers to the ordinary callable, the keyword implementation/sorter machinery,
and `Core.kwcall`; publication is still one source definition event whose
methods become visible at the same top-level chronology point. Relevant
authorities are `julia/src/method.c::jl_method_def`, the `primary_world` handling
in `julia/src/gf.c`, and keyword lowering in
`julia/src/julia-syntax.scm`.

sjulia's current IR already normalizes each user declaration to one source
`Function` and one `DefineEvalFunction` marker. A release bytecode probe for
`f(x::T) where T = x` and `g(x; k=1) = x+k` produces one declared body for each
method and one marker for each. Generated specialization and closure rows are
parallel runtime metadata, not separate source definitions. Therefore the
design extends the existing source-method activation transaction instead of
introducing a second keyword-family publication protocol.

## Chosen Architecture

### Eligibility is semantic, extraction is structural

Remove `type_params.is_empty()` and `kwparams.is_empty()` as syntactic reasons
to reject an input in `REPLSession::input_defines_only_new_generic_functions`.
The semantic gate continues to reject Base/preload-owned generics and unrelated
lowering-generated functions. The compile-side relocatable extraction remains
the independent structural authority: every body, call target, specialization
row, global slot prefix, and activation member must be proven aligned before
the held VM is mutated.

This separation avoids maintaining a second list of Julia syntax forms. A valid
source method is admitted based on ownership; any compiler layout that cannot
yet be extracted returns `Ok(None)` without changing session state. Acceptance
tests nevertheless require all supported parametric/keyword shapes in this
design to pass extraction and report VM build time zero, so fallback is not
treated as success.

### Identity and dependency refresh

`ReplMethodIdentity` remains the canonical identity across source snapshots,
method tables, specialization rows, and activation rollback. Its structured
signature must distinguish `where` constraints, positional signatures, and the
keyword-call surface without using display strings or function-name-only keys.

For an extension or replacement, `ReplMethodSourceSnapshot` computes the same
marker-specific transitive caller refresh plan used by ordinary Main methods.
The source method and all refresh bodies form one
`ReplDefinitionActivation::FunctionGroup`; the VM publishes them at one world
increment. A caller redefined before or after the mutation in the same input is
compiled from the exact method prefix visible at that marker.

### Keyword metadata and specializations

Keyword defaults and supplied-keyword dispatch remain represented by the
source `Function.kwparams` plus the compiler's existing specialization rows.
Every pre-existing specialization row whose canonical method identity matches
the mutated source is replaced at activation with the new IR and new fallback
index. Newly emitted rows are installed dormant only when they belong to a
source activation group; marker-free inline closures remain immediately active.

The design must cover omitted defaults, supplied keywords, multiple keywords,
keyword splats, varargs, bounded `where` parameters, repeated type variables,
and a method combining `where` with keywords. It must not special-case method
names, packages, or individual signatures.

## Transaction and Error Semantics

Compilation is prepare-before-commit. Failure of ownership checks, structural
extraction, identity matching, or alignment returns to the existing full path
without taking or modifying `live_vm`.

After installation, each source marker publishes its primary and refresh group
atomically. On a Julia-catchable error, the VM reports the reached definition
prefix. `ReplPersistentCompile::retain_reached_function_prefix` projects method
sources, method tables, dependency edges, function-name visibility, and
specialization updates to that same prefix while retaining dormant positional
bodies only for index alignment. An unreached parametric/keyword method must be
absent from later dispatch and reflection. Host cancellation and internal
invariant errors continue to drop the live VM.

## Verification

Tests are added to the existing consolidated REPL differential/session binary;
no new test binary is created.

The red/green matrix includes:

1. brand-new `where`, keyword, and combined `where`+keyword definitions;
2. new-signature extension and same-signature replacement;
3. omitted and explicitly supplied keyword defaults, multiple keywords,
   keyword splat, positional varargs, bounded and repeated type variables;
4. direct, resolved, dynamic, and specialized calls where reachable;
5. a transitive caller chain and both caller-before/caller-after marker orders;
6. one reached replacement followed by an error and one unreached extension;
7. a marker-free inline closure after a keyword/parametric mutation;
8. upstream Julia output parity for every fixture row; and
9. `last_vm_build_nanos() == Some(0)` for every definition mutation covered by
   this design.

Focused release-fast tests and REPL differential tests run first. Final gates
are formatting, source-only audits, default clippy, release sjulia, iOS device
and simulator builds, the full release nextest suite, metamorphic equivalence,
and guarded regular merge.

## Rejected Alternatives

### Introduce a new keyword definition-family ID

This mirrors upstream lowering artifacts too literally after sjulia has already
normalized them into one source `Function`. It would create a second identity
and rollback protocol without evidence that multiple source markers exist.

### Start with type or module registry append

Those are required later for #9784, but they do not remove the simpler LV3b
fallback adjacent to the method-world transaction that just landed. Completing
the function-definition family first reduces the remaining fallback surface
before the larger LV4b/LV5b registry work.

## Follow-on Order Within #9784

After this slice, continue the same oldest Issue in dependency order:

1. live append for abstract/primitive/enum and parametric/inner-constructor
   types, including source-ordered rollback;
2. module-definition/redefinition and module-owned definition activation;
3. package `using`/inner import/macro/type-alias/baremodule state;
4. opaque runtime `eval` routing through the normal compile/VM transaction; and
5. deletion of global/module reconstruction mirrors and accumulated-source
   full-recompile machinery, followed by closing #9784.
