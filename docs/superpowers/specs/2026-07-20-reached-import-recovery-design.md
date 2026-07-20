# Reached selective-import recovery design

Issue: #11748

## Problem

A selective import executes before an uncaught REPL error, while a second import
is source-later and unreached. Upstream retains only the reached binding. sjulia
loses the reached import immediately after the error, before any full-rebuild
barrier.

## Investigation

The compiler currently lowers each `Stmt::Using` to runtime import-metadata
writes, but unlike methods and runtime nominals it emits no statement-level
activation event. Those writes identify affected bindings, not the completed
`UsingImport`: they cannot represent a zero-binding/no-op import and cannot
unambiguously reconstruct aliases or an import that leaves metadata unchanged.
Consequently the catchable-error path has no exact evidence with which to
project `Program.usings`; storing the whole vector would also revive the
source-later import.

Give every resolved using/import its owning module path and originating local
`usings` index; a bare index would collide between Main and nested modules.
After all runtime metadata for that statement has been emitted, emit a dedicated
bytecode activation carrying that identity. The VM records activations reached
by the current appended main and clears the trace at each re-entry. At a
catchable toplevel error, select Main-owned events, validate their indices (in
range, unique, source ordered), and store only those exact `UsingImport` entries
through the existing distinct-import policy. Successful evaluations continue
to use the ordinary all-import commit path.

The compiled VM reserves an import's static binding surface before the runtime
marker executes. When the trace proves that any import statement is unreached,
the errored VM therefore cannot remain the next eval's authority: otherwise
`isdefined` can observe a dormant source-later binding. Recovery first projects
the VM's reached value/module state into the host mirror, stores the exact
reached imports, and then drops that VM so the next eval rebuilds from the
sanitized session state. When every import marker ran, normal live-VM reuse
remains available.

This is statement identity rather than binding-name inference, so it covers
selective imports, whole-module imports, aliases, ambiguity/no-change cases,
and zero-binding imports with one mechanism. A cache schema bump is required
because the bytecode enum gains an instruction.

## Verification

Run the new RED/GREEN test, import/module recovery neighbors, #11740 matrix,
source audits, fmt, clippy, and the guarded full suite.
