# Semantic-ID Phase 4 TypeVar Classification Design

## Problem

Issue #10992 requires `typevar_core_bindings` to reach zero or for every
residual to be proven lexical. The current source-only audit only ratchets a
raw count of 14 `HashMap<String, CoreType>` spellings. Its prose explains that
the sites are either rendered-type parse caches or single-dispatch lexical
`where` substitutions, but the Rust types and the audit do not encode that
distinction. A new semantic map can therefore replace an existing lexical map
without changing the count and still pass.

The struct half of Phase 4 is already complete: `StructRegistry` retired
name-keyed layout tables and Issue #11046 retired the 19 bare fallback
decisions. The next bounded slice is therefore to make the TypeVar verdict
structural and enforceable before undertaking #11095's function/method table
migration.

## Chosen approach

Introduce two private type aliases with deliberately narrow meanings:

- `LexicalTypeBindings = HashMap<String, CoreType>` is owned by the shared
  dispatch resolver and exists only for one candidate match. Its string key is
  a source-level `where` binder spelling used to resolve into structured
  `CoreType` values; it is never persisted or shared between candidates.
- `RenderedTypeParseCache = RefCell<HashMap<String, CoreType>>` is owned by
  `type_core.rs`. Its key is the complete rendered input to a pure parser. It
  is a memoization boundary, not an identity table.

All 14 direct spellings move behind those two aliases. The audit then excludes
only the exact alias declarations and holds every other direct
`HashMap<String, CoreType>` spelling at zero. Required anchors make deletion or
renaming of either classified authority fail closed. This is stronger than a
count baseline: replacing a lexical use with an unclassified map is detected
even if another site disappears in the same change.

The aliases do not change runtime representation, serialization, dispatch
selection, or cache behavior. A wrapper/newtype was considered but rejected
for this slice because it would require broad forwarding implementations while
adding no stronger ownership guarantee than the private module boundary and
the zero-unclassified-site audit.

## Components

### Shared dispatch lexical bindings

`subset_julia_vm_types/src/inference_core/dispatch_resolver.rs` defines the
private alias. Its `core_match` child imports the alias instead of importing
`HashMap`, and every signature/local accumulator in both modules uses the
alias. Existing matcher tests continue to prove diagonal binding, bound
checking, and conflict-aware merge behavior.

### Rendered-type parse cache

`subset_julia_vm_types/src/inference_core/type_core.rs` defines the private
cache alias and uses it for both thread-local caches and the parsing closure.
The two caches stay separate because dispatch parsing preserves qualified
nominal owners while ordinary type parsing retains its historical
canonicalization.

### Failing audit and inventory

`scripts/check_name_based_lookup.sh` classifies the two exact alias
declarations, reports `typevar_core_bindings` as the count of unclassified raw
sites, and sets its baseline to zero. A negative self-test replaces one
`LexicalTypeBindings` use with the raw map type and must make the audit fail.

`scripts/semantic_id_inventory.py` mirrors the audit's exclusion rules so its
live reconciliation remains exact. The two aliases appear as explicit lexical
boundary rows, while the former 14-site identity-bearing anchor becomes zero.
The committed TSV is regenerated deterministically.

## Documentation and Issue state

`SEMANTIC_IDENTITIES.md` and `SEMANTIC_ID_MIGRATION.md` record the TypeVar
criterion as complete and correct the stale Phase 2b text: struct lookup debt
is zero, not 19. `CODE_AUDITS.md` documents the zero-unclassified-site contract.
After merge, #11078 can be closed with links to the landed StructId re-key,
owner-scoped resolver, negative audit, and fresh/cache parity evidence.

#10992 remains open because #11095's function/method identity-bearing tables
and #11089's using-scope visibility defect are independent remaining work.

## Verification

- Run the negative self-test first and observe the new raw-map mutation fail.
- Run the focused semantic inventory unit tests and deterministic regeneration.
- Run `bash scripts/check_name_based_lookup.sh` and the complete source-only
  audit suite.
- Run `cargo test -p subset_julia_vm_types inference_core::dispatch_resolver`
  and `cargo check -p subset_julia_vm --features repl`.
- Run formatting and the repository clippy lanes before the PR.
- Because Rust source changes, run the full release nextest suite through the
  guarded premerge gate before regular merge.

## Non-goals

- Re-keying function or method tables; that remains #11095.
- Fixing using-scope method visibility; that remains #11089 and is the next
  behavioral slice.
- Introducing `FunctionId` or `MethodId` without a production consumer.
- Changing Julia-visible names, diagnostics, dispatch results, or cache wire
  formats.
