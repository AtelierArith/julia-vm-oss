# Semantic-ID As-Landed Verdict Reconciliation Design

## Problem

Issue #10459's semantic-ID plan was written before the Phase 2a, 2b, and 3
implementation investigations completed. The current documents still present
several disproved assumptions as future work: all 117 module/global sites were
expected to migrate to `ModuleId`, the struct registry was expected to require
a serialized relocation table, and `FunctionId`/`MethodId` were expected to be
introduced before Phase 4. The landed work reached different, evidence-backed
conclusions in PRs #11033, #11084, #11098, #11156, and #11191.

The committed `SEMANTIC_ID_INVENTORY.tsv` is also only a mechanical count. It
cannot distinguish a semantic identity table from a sanctioned lexical
name-to-ID boundary or a table proven inert for identity decisions. As a
result, Issue #10992 still treats every name-shaped site as equivalent debt and
cannot define an honest Phase 4 retirement target.

## Chosen approach

Reconcile the two design documents and extend the existing report generator in
one PR. This keeps the prose verdicts and the machine-readable inventory on one
reviewable commit lineage. A docs-only reconciliation would leave Phase 4
without a measurable target; a new enforcement gate would be premature because
the identity-bearing residual remains intentionally nonzero under #11078 and
#11095.

The generator gains a `verdict` dimension with exactly three values:

- `identity-bearing`: a site whose string key or lookup can still choose,
  compare, store, or recover semantic identity and therefore remains Phase 4
  work;
- `lexical-boundary`: a sanctioned source/display name to canonical path or ID
  resolution boundary;
- `inert`: a mechanically matched site that has been verified not to decide
  semantic identity.

Classification is conservative. Every site in the six core domains defaults to
`identity-bearing`. Explicit, reviewer-auditable rules may downgrade a site only
when a landed PR provides evidence that it is a lexical boundary or inert. The
`other` domain also defaults to `identity-bearing`: the existing inventory docs
warn that mechanical domain classification has false negatives such as
function-name call graphs. Phase 4 totals filter to #10459's six core domains,
while the unadjudicated `other` sites remain visible without being mislabeled
inert. This avoids silently excluding unreviewed debt.

## Verdict rule ownership

The rule table lives beside the existing domain and difficulty rules in
`scripts/semantic_id_inventory.py`. Rules match the discovered file, symbol,
kind, and domain before module-level aggregation. The committed TSV aggregates
on `(kind, domain, layer, difficulty, verdict, module)`, so one module may
produce separate rows for identity-bearing, lexical-boundary, and inert sites.

The initial explicit rules encode the per-table findings from PR #11191:

- `module_functions`, `module_exports`, `module_constants`,
  `module_struct_names`, `module_usings`, and `module_abstract_names` are
  canonical qualified-path lookup boundaries;
- `module_imported_bindings` is an injective qualified-module-plus-symbol
  lookup boundary;
- `module_aliases` is a lexical import alias boundary whose source-order bug
  was fixed by #11176;
- `global_types`, `inference_global_types`, `global_const_structs`, and
  `global_struct_names` are inert for identity selection under the documented
  widening/dynamic-resolution behavior;
- the `ModuleInternTable` and `StructRegistry` name indexes are lexical
  name-to-ID boundaries, not parallel identity stores.
- `TypeVarScope.by_name` is the documented lexical name stack used to locate
  scoped `CoreTypeVar` identities; it is distinct from the residual
  `typevar_core_bindings` maps and is a lexical boundary.

No broad path-only downgrade is allowed. Function/method tables, residual
TypeVar binding maps, and the 19 bare struct lookup anchors therefore remain
`identity-bearing` until #11095, #10460, or #11078 retires or explicitly
reclassifies them.

## Documentation reconciliation

`SEMANTIC_ID_MIGRATION.md` will replace the speculative phase table with an
as-landed table containing `planned`, `landed`, and `verdict` columns. Disproved
claims are labeled `REFUTED` with the PR that supplied the evidence, rather
than retained as historical-looking future requirements.

The resulting phase record is:

- Phase 2a: `ModuleId` and the persisted `macro_bindings` relocation path
  landed; the 12 originally named tables were classified rather than re-keyed.
- Phase 2b: `StructId`/`StructRegistry` re-keying landed using Pattern A
  (derive, do not serialize); owner-aware bare resolution remains #11078.
- Phase 3: `FunctionId`/`MethodId` were not introduced because no consumer
  would retire a table; the real remaining resolver/table work is #11095.
- Phase 4: count only `identity-bearing` residuals, while keeping every
  nonzero residual linked to its owning continuation Issue.

`SEMANTIC_IDENTITIES.md` will mirror those statuses and state that the target
model is descriptive rather than a mandate to introduce unused IDs. Its Phase
4 checklist will use verdict-aware residuals instead of requiring every
name-shaped table to reach zero.

After the PR merges, Issue #10992's body will be rewritten to the same Phase 4
contract: retire `identity-bearing` residuals, require explicit evidence for
every downgrade, and exclude the already adjudicated lexical/inert tables from
the zero target. This external issue edit is part of #11284 completion but does
not belong in git history.

## Verification

The generator will expose verdict totals in its console summary and include a
domain-by-verdict cross-tab. Focused Python tests will cover conservative
defaults, each explicit downgrade family, and aggregation separation when one
module contains multiple verdicts. Regeneration must be deterministic and the
six existing `check_name_based_lookup.sh` anchor counts must continue to match
exactly.

Required checks for the implementation PR are:

- focused verdict-classification tests;
- `python3 -m py_compile scripts/semantic_id_inventory.py`;
- two independent inventory regenerations with byte-identical TSV output;
- `bash scripts/check_name_based_lookup.sh`;
- `bash scripts/run_source_only_audits.sh`;
- `git diff --check` and the guarded PR gate.

The change does not alter Rust, Julia semantics, cache schemas, AoT, FFI, or
fixture behavior, so a VM full-suite run is not required unless the final diff
expands beyond the documented script/docs scope.

## Non-goals

- Retiring the #11078, #11095, or #10460 identity-bearing residuals in this PR.
- Promoting the report generator into a failing source-only audit before Phase
  4 has reduced and stabilized its residual set.
- Introducing an ID type with no production consumer.
- Treating a qualified-path lexical lookup as semantic identity debt solely
  because its physical key type is `String`.
