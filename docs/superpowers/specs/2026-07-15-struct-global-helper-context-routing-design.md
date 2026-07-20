# Struct-global helper context routing design (#11179)

## Problem

PR #11136 added `global helper(...) = new(...)` support inside struct bodies,
but lowered those helper functions through context-free entry points. That
violates the repository's LambdaContext routing authority and loses lifted
functions created inside a helper. The same PR also increased two structural
debt inventories without reconciling their ratchets, leaving `main` unable to
pass the guarded source-only gate.

Adversarial review exposed two adjacent correctness gaps before merge. Helpers
stored on a `StructDef` were never extracted on transparent-block or `@kwdef`
paths (#11186), and the shared `new` seam collapsed any mixed/multiple splat to
only its final argument (#11183/#11187).
The same post-hoc owner stamping crossed a semantic boundary: a global function
created by runtime `@eval` inherited the enclosing struct helper's privileged
`new` authority, while upstream resolves `new` in the eval target module
(#11197). A nested anonymous closure correctly lost authority but initially
compiled an unresolved direct call into `ErrorException("Unknown function")`
instead of performing ordinary lookup and raising `UndefVarError` (#11204).
Missing-binding and user-binding regression cases live in separate source files:
sjulia still exposes a generic's first binding before its textual definition,
which is the distinct source-world bug tracked by #11210.

## Design

Keep the context-free `lower_struct_definition` API for genuinely context-free
callers, and add `lower_struct_definition_with_ctx` for every lowering path that
already owns a live `LambdaContext`. Thread the optional context through struct
body parsing and route full/short global helpers through the existing
`lower_*_with_ctx_if_needed` authorities. The `None` branch remains explicit so
the audit can distinguish a proven context-free path from an accidental bypass.

Install the enclosing struct's `new` authority as lexical state in
`LambdaContext` while lowering each global helper. Stamp each lifted function
at creation time from that state, and temporarily clear it while lowering a
runtime `@eval` function. This preserves Julia's lexical privilege for ordinary
closures while also excluding lifted async/task/macro thunks beneath an eval
boundary; a post-hoc watermark cannot distinguish those descendants.

Use one helper-extraction authority for direct, module, transparent-block and
`@kwdef` structs. It removes `global_new_helpers` from the definition, stamps
and registers the functions when a live context exists, and appends them to the
enclosing source function list.

Keep `Expr::New`'s whole-list splat representation. When any source argument is
splatted, first build an ordinary `tuple(args...)` call with the complete source
splat mask, then pass that single tuple to `new` as a whole-list splat. This
preserves evaluation order, leading arguments, and multiple splats without a
new IR/wire schema.

Carry the struct owner as traversal state while collecting ordinary lexical
function descendants. Clear it at every `EvalFunctionDef`, for both the eval-
defined global function and descendants nested inside it. If an `Expr::New`
reaches compilation without an authorized owner, compile it as an ordinary
indirect call through the ordinary binding named `new`; this preserves upstream's catchable
`UndefVarError` and permits an explicit user binding instead of aborting the
entire program at compile time. This replaces slice-wide post-hoc stamping,
which cannot represent a boundary inside the collected function sequence.
Retain explicit type parameters through `ApplyTypeDynamic`. Because privileged
inner-constructor `new` rejects keywords upstream, lower every keyword-bearing
`new` spelling directly as an ordinary indirect call, preserving keyword values
and keyword splat masks without widening the compact `Expr::New` schema.

Extend the existing #11005 fixture with a parametric helper whose immediately
invoked closure calls `new{T}`, the transparent begin/let/assignment-RHS and
`@kwdef` extraction paths, and a non-parametric constructor with leading and
multiple splats. Add an eval-defined global function negative control beside an
ordinary lexical nested-function positive control. Verify it first with upstream
Julia, then with sjulia.
Reconcile the two intentional #11136 inventory increases at their exact observed
counts, with comments identifying the feature and test growth; do not hide any
other debt category.

## Verification

- upstream Julia parity for `global_new_helper_11005.jl`
- `scripts/run_source_only_audits.sh`
- targeted fixture nextest, fmt, clippy
- guarded full-suite premerge gate before regular merge
