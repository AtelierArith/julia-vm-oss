# Try-clause soft-scope provenance design

Issues: #11322, #11305, #11335, #11331 (cluster 1); partial #11159

## Problem

The compiler now gives each `try`/`catch`/`else`/`finally` clause a distinct
lexical owner, but the strict file-mode soft-scope pass still treats a top-level
`Stmt::Try` as scope-transparent in two places:

- `record_toplevel_globals` records fresh clause assignments as module globals,
  so a later top-level loop emits a phantom ambiguity warning (#11322).
- `process_toplevel_stmts` does not apply the clause's own soft-scope decision.
  Consequently an assignment to an existing mutable global mutates the global
  without the upstream warning (#11335).

The #11305 MWE is already green on current `origin/main` because the loop-body
assignment collector stops at `Stmt::Try`; it has no dedicated regression and
can regress if either walker becomes scope-transparent again.

## Semantic contract

1. A fresh assignment owned by a try clause does not
   create a module-global-before fact for later strict soft-scope decisions.
2. An existing mutable global assigned by a top-level clause is localized with
   the strict soft-scope warning; the outer global remains unchanged (#11335).
3. An existing `const` is localized silently; the outer const remains unchanged
   both in a direct loop and through the #11305 nested-clause shape.
4. An explicit `global` declaration executed inside a clause still identifies a
   module binding and is visible to later top-level soft scopes.
5. Sibling clauses do not share fresh bindings. An ordinary assignment in a
   nested try reuses an already-localized enclosing clause slot (#11159), while
   an explicit local/global or catch binder still shadows it.
6. A later real module assignment supersedes a retired clause-local fact; the
   provenance inventory is source ordered, not an accumulating union.
7. A try clause nested in an outer top-level loop remains a lexical boundary,
   so a same-named outer global does not cause the #11305 warning.

## Design

Replace the top-level try special case with one source-ordered binding authority:

- retain `global`, `const`, and retired clause-local provenance separately;
- keep live global/const and retired-local states mutually exclusive so a later
  top-level assignment wins;
- derive assignment-backed clause bindings from `ScopeBindingInventory` so
  function/generic identities are not rewritten as value slots;
- localize mutable globals with a warning, consts without a warning, and retired
  clause names without a warning before walking nested clauses/loops;
- teach the source-order recorder to expose only explicit globals from a
  `Stmt::Try` while retiring ordinary clause-local spellings.

The explicit-global collector is deliberately separate from
`ScopeBindingInventory`: the inventory describes one lexical owner and stops at
nested hard scopes, while this collector answers whether an executed prior
statement can establish a module binding for later source-order decisions.

This bounded slice does not make that collector execution-aware. An explicit
global in an untaken clause still produces a phantom later warning (#11338), and
a try nested below an arbitrary value expression remains outside the direct
statement walker (#11159). A direct fresh loop assignment also still escapes its
loop lifetime unless retired provenance forces a rename (#11339). All reproduce
on the exact pre-change main tree and remain tracked rather than being hidden by
special cases here.

## Verification

- Lowering unit tests first demonstrate the #11322 provenance leak and #11335
  missing localization as RED.
- The matrix covers fresh try binding, explicit global, nested reuse, later
  global precedence, nested loop ownership, and the #11305 const-shadow shape.
- CLI stderr tests prove no warning escapes end to end for #11322/#11305 while
  preserving the warning for a genuine explicit global.
- Run lowering tests, consolidated CLI/scope tests, source-only audits, format,
  Clippy, and the guarded full release suite before regular merge.
