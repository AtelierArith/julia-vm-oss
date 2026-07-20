# Source-Ordered Type Alias Signatures Design

## Problem

SubsetJuliaVM pre-scans every type alias before lowering statements. That is
necessary for static canonicalization, but it makes a later alias visible in an
earlier method signature. Upstream Julia evaluates the annotation when the
method definition executes and raises `UndefVarError` for that forward
reference (Issue #11086).

Keeping the bare alias spelling is not a valid fix. Method dispatch depends on
canonical alias expansion, especially for aliases declared inside modules.
The alias registry also currently stores one entry per string key, so a sibling
module's later pre-scan can overwrite the entry that another module should use.

## Chosen architecture

The lowering-time alias registry will keep identity-bearing entries rather than
one lossy value per bare key. Each pre-scanned entry records:

- its lexical module owner;
- a unique identity for the source fragment in which it was declared;
- its definition byte position within that source;
- its registration order, used to choose the newest available definition.

Every lowering invocation creates a fresh source identity. Includes, loaded
packages, and later REPL evaluations therefore never compare their byte offsets
with the current source. Snapshot/restore preserves outer entries while source
scope guards preserve the caller's identity across nested lowering.

Pre-scan recursion carries the module path explicitly and appends entries to a
per-name history. It does not overwrite a bare entry when it encounters a
sibling module or `Main` alias with the same leaf name. Resolution first chooses
the entry visible for the active lexical module or explicit qualification, then
applies source-order availability to that selected entry.

## Resolution modes

Ordinary alias expansion remains the canonical mode. It selects the newest
module-correct entry and recursively expands its target exactly as before. This
preserves constructor, type expression, module-local method dispatch, imported
alias, and cache-facing canonical type behavior.

Function signature parsing uses a source-ordered mode. Given the annotation's
span, an entry is unavailable only when both conditions hold:

1. the entry and annotation have the same explicit source identity; and
2. the entry's definition starts after the annotation use.

An unavailable current-source candidate is skipped so an earlier definition
from a prior REPL evaluation can still be selected. Entries from any different
source are treated as already available; their unrelated numeric spans are
never compared. If no entry is available, the existing undefined-annotation
probe receives the unexpanded name and produces Julia-compatible
`UndefVarError` behavior.

Alias exclusion for `where` and struct type parameters remains authoritative
before either resolution mode.

## Module selection

Entries retain their full lexical owner. A bare use in a module prefers the
same-owner entry; a qualified use prefers its exact owner. Existing unique-leaf
fallback remains available for imported/qualified compatibility only after
owner-exact selection. Ambiguous sibling leaves must not select whichever entry
was pre-scanned last.

The active module path is a scoped lowering context, so nested module lowering
restores its parent path even on errors. Pre-scan uses its own explicit module
path and therefore cannot confuse discovery order with lexical visibility.

## Cache and REPL behavior

Source identity and definition positions are transient lowering metadata. They
do not enter `Program`, `MethodSig`, bytecode, or serialized caches, so no cache
schema invalidation is required. Cache correctness is verified by running the
same signature/alias matrix through cold, primed, and cached execution lanes.

REPL/shared-context lowering retains alias history across evaluations. A prior
evaluation's alias is visible in a later signature, while a redefinition later
in the same new evaluation does not become visible before its definition.

## Tests

The RED fixture and focused tests cover:

- top-level later alias in an earlier signature (`UndefVarError` parity);
- an earlier top-level alias that still dispatches canonically;
- module-local aliases in sibling modules and `Main` that share one leaf name;
- qualified and imported aliases;
- nested alias arguments;
- prior REPL evaluation versus a later same-fragment redefinition;
- cold, prime, and cached execution parity.

Unit tests exercise source identity equality, same-source order filtering,
different-source non-comparison, module-owner selection, and snapshot/source
scope restoration. The fixture ends with `true` and is registered in the
existing consolidated fixture binary.

## Non-goals

- Sequentially rewriting the whole lowering pipeline.
- Deferring all alias resolution to runtime.
- Changing canonical alias representation in compiled IR or bytecode.
- Adding package-name or module-name special cases.
