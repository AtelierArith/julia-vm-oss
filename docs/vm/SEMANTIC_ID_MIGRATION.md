# Semantic-ID Migration Plan (Issue #10459 Phase 0)

Issue #10459 is the owner epic for retiring bare-name identity tables and
replacing them with owner-scoped semantic IDs (`ModuleId`, `TypeVarId`,
`StructId`, `FunctionId`, `MethodId`) — as opposed to
`scripts/check_name_based_lookup.sh`, which is Issue #10279's bug-cluster
guard: it ratchets six *already-known* patterns so they cannot silently grow,
but does not itself retire the debt or cover sites outside those six
patterns. This document is the **Phase 0** deliverable: a mechanical,
re-runnable inventory of every bare-name identity site in production code
(not just the six ratcheted patterns), a dependency-ordered migration plan,
and a hand-off of Phases 1-4 to tracked sub-issues.

`docs/vm/SEMANTIC_IDENTITIES.md` remains the higher-level design record (the
target `ModuleId`/`TypeVarId`/`StructId`/`FunctionId`/`MethodId` model,
identity vocabulary, and the review checklist). This document is its Phase 0
companion, in the same relationship `docs/vm/PANIC_DEBT_RETIREMENT.md` has to
`docs/vm/PANIC_FREE.md` for Issue #10869.

## Related epics — what this epic does NOT own

Three sibling design epics touch adjacent territory. None of their scope is
duplicated or owned here; each boundary is stated explicitly so a future PR
against any of the four does not have to re-derive it:

- **#10460** (preserve structured `UnionAll`/`TypeVar` semantics end-to-end;
  retire type-string reparsing) is about a *different* string dependency:
  re-parsing a rendered type string back into a `JuliaType`/`CoreType` at a
  later pipeline stage, losing structure in the round trip. #10459 is about
  a rendered name used as a `HashMap` *key* for identity/equality, not about
  reparsing a type string into a value. The two overlap only where a
  `typevar`-domain site's key string is *also* something #10460 would want
  to stop reparsing (e.g. a cached projection's display name) — in that
  narrow overlap, #10460 owns "stop treating the string as a serialized
  type", #10459 owns "stop treating the string as the identity key". Phase 1
  completion (#10987) touches exactly this overlap and should coordinate
  with #10460 rather than duplicate its analysis.
- **#10461** (route direct/callable/HOF/specialized calls through one
  semantic resolver) overlaps most directly with this epic's own required
  property #3 ("qualified and bare lookup resolve a name to an ID; they do
  not use the name as the identity") and with Phase 3 (#10990,
  `FunctionId`/`MethodId`). #10461 owns *how many call-dispatch code paths
  exist* and whether they agree with each other; #10459 owns *what a
  function/method's identity is once resolved*. A unified resolver (#10461)
  still needs a canonical target, but PR #11098 proved that an unread
  `FunctionId`/`MethodId` is not automatically that target. #11095 owns the
  remaining identity-bearing table/resolver sites and must reuse #10461's
  semantic call target rather than introduce a parallel ID path.
- **#10436** (`where`-binder scoping/environment) already owns the scoping
  and allocation flow that feeds TypeVar identity — `SEMANTIC_IDENTITIES.md`
  says so explicitly ("owner-scoped IDs are allocated and made visible
  through the shared lexical environment described in
  `WHERE_BINDER_ENVIRONMENT.md` ... This document owns the identity model;
  the binder-environment doc owns the scoping and allocation flow"). #10459
  consumes #10436's binder environment; it does not re-implement binder
  scoping.

## Scope disclaimer — read this before drawing conclusions from the numbers

This document (and `scripts/semantic_id_inventory.sh`) measures exactly
three mechanical shapes: `HashMap`/`BTreeMap<String, X>`-or-`<&str, X>`
declarations (`map_decl`), `*_by_name` identifiers (`by_name_ref`), and the
six patterns `scripts/check_name_based_lookup.sh` already ratchets
(`anchor`; as of Issue #10987, `EXTRA_ANCHOR_ROWS` no longer adds a
hand-declared row on top of those six — see "Headline counts" below).
**This is not a semantic-collision proof.** A site classified `other` (281
of 837, 34%) is not "safe" — it is "the keyword scan could not confidently
place it in one of the six named domains", e.g. Julia-level
`getfield`-by-Symbol reflection helpers, macro-name tables, and
closure-capture sets (see the script's module docstring "Domain
classification" section for the full list of known `other` classes). A site
classified `mechanical-rename` is not "zero-risk" — it means the map's *key
type* can be swapped without redesigning callers, not that the surrounding
design work is free. See the script's "Known limitations" section for the
heuristic gaps: some multi-line declarations fall back to the literal
`HashMap`/`BTreeMap` token as their symbol name. Verdict classification
therefore also fails closed; a site is `identity-bearing` until an exact
landed-evidence rule proves it lexical or inert.

## How to regenerate

```bash
bash scripts/semantic_id_inventory.sh
bash scripts/semantic_id_inventory.sh --detail /tmp/semantic_id_detail.tsv  # per-line audit aid, not committed
```

This is a **report generator, never a gate** — it always exits 0, is not
wired into `premerge_gate.sh` or `check_name_based_lookup.sh`, and rewrites
`docs/vm/SEMANTIC_ID_INVENTORY.tsv` (aggregated `kind, domain, layer,
difficulty, verdict, module -> count`, the same granularity as
`docs/vm/PANIC_DEBT_CLASSIFICATION.tsv`) plus a stdout summary. See the
script's module docstring for the full classification mechanism (test-only
exclusion machinery shared with `scripts/panic_debt_classification.py`,
comment/string masking, the `enclosing_block_kind` brace-depth scan for
migration-difficulty context, and the fixed-order domain-keyword rules).

## Headline counts (2026-07-15, regenerated for Issue #11284)

The current snapshot contains **837** classified sites: 700 `map_decl`, 105
`by_name_ref`, and 32 `anchor`. The six core domains account for 556 sites;
281 mechanically fall into `other` and remain visible/fail-closed. All six
anchor patterns reconcile exactly with `scripts/check_name_based_lookup.sh`.

### By identity domain

| Domain | Sites | Existing `anchor` sites | Non-anchor sites |
|---|---:|---:|---:|
| typevar | 15 | 13 | 2 |
| struct | 272 | 19 | 253 |
| function | 95 | 0 | 95 |
| method-sig | 30 | 0 | 30 |
| module | 81 | 0 | 81 |
| global | 63 | 0 | 63 |
| **6-domain total** | **556** | **32** | **524** |
| other (visible, outside Phase 4 total) | 281 | — | — |
| **grand total** | **837** | | |

### By semantic verdict (six core domains)

| Domain | identity-bearing | lexical-boundary | inert | Total |
|---|---:|---:|---:|---:|
| typevar | 14 | 1 | 0 | 15 |
| struct | 271 | 1 | 0 | 272 |
| function | 95 | 0 | 0 | 95 |
| method-sig | 30 | 0 | 0 | 30 |
| module | 25 | 56 | 0 | 81 |
| global | 16 | 0 | 47 | 63 |
| **Phase 4 residual** | **451** | **58** | **47** | **556** |

Every site defaults to `identity-bearing`. The 58 lexical and 47 inert sites
are exact symbol/path downgrades backed by #11191 or by the existing
`ModuleInternTable`/`StructRegistry`/`TypeVarScope` name-to-ID contracts.

### By migration difficulty (six core domains)

| Domain | mechanical-rename | requires-owner-context-plumbing | requires-serialization-format-change | Total |
|---|---:|---:|---:|---:|
| typevar | 0 | 15 | 0 | 15 |
| struct | 15 | 254 | 3 | 272 |
| function | 12 | 79 | 4 | 95 |
| method-sig | 0 | 27 | 3 | 30 |
| module | 4 | 67 | 10 | 81 |
| global | 0 | 51 | 12 | 63 |
| **total** | **31** | **493** | **32** | **556** |

### By layer (six core domains)

| Layer | typevar | struct | function | method-sig | module | global | Total |
|---|---:|---:|---:|---:|---:|---:|---:|
| compile | 0 | 136 | 33 | 17 | 68 | 36 | 290 |
| vm | 1 | 55 | 28 | 8 | 2 | 8 | 102 |
| inference | 14 | 47 | 0 | 0 | 0 | 0 | 61 |
| cache | 0 | 3 | 4 | 3 | 10 | 12 | 32 |
| lowering | 0 | 1 | 0 | 0 | 1 | 0 | 2 |
| other | 0 | 30 | 30 | 2 | 0 | 7 | 69 |

`compile` remains the dominant surface, but layer/difficulty are planning
metadata rather than verdicts. Phase 4 is governed by the 451-site
identity-bearing residual, not by physical `String` shape alone.

## Cache-serialization impact (32 `cache`-layer sites)

| Domain | Cache-layer sites | Representative fields |
|---|---:|---|
| global | 12 | `global_types: HashMap<String, ValueType>`, `global_struct_names: HashMap<String, String>` (`compile/cache.rs`) |
| module | 10 | adjudicated lexical module tables plus identity-bearing preload/module-public bookkeeping (`compile/cache.rs`, `compile/preload_cache.rs`, `loader.rs`) |
| method-sig | 3 | `source_ordered_method_sigs`-shaped tables restored from cache |
| function | 4 | function-table restore paths (`take_prefetched_base_function_table`, `export_base_cache`) |
| struct | 3 | `type_aliases` and `parametric_structs` cache-boundary declarations; the old `HashMap<String, StructInfo>` table is gone |

> **REFUTED (PR #11156):** the original lower-bound analysis inferred that
> `StructInfo` crossed a wire format because the struct table is rebuilt from
> cached definitions. The implementation audit proved `StructInfo` derives no
> `Serialize` and `RuntimeCompileContext` is `#[serde(skip)]`; the registry is
> Pattern A (derive on both lanes), not a relocation surface. Cache obligations
> are decided per field: Pattern B only when the identity-bearing field itself
> is serialized.

## Migration phases — as-landed authority

The table below is the current authority. Later sections retain the original
investigations as history, with explicit callouts where a continuation PR
refuted or superseded their premise. A typed ID is a means of retiring a
semantic identity decision, not a deliverable when no production consumer
would read it.

| Phase | Planned | Landed evidence | Verdict / remaining owner |
|---|---|---|---|
| 1 — TypeVar | Retire rendered-name identity keys. | Structural `CoreTypeVarId` and projection keys landed; runtime rendered-name keys are zero (#10987). The 14 raw `HashMap<String, CoreType>` spellings are now two exact private authorities: `LexicalTypeBindings` and `RenderedTypeParseCache` (#10992). | **migrated/classified**. Unclassified `typevar_core_bindings` is zero; both remaining declarations are mutation-tested lexical boundaries. Structured representation work outside those boundaries remains #10460. |
| 2a — Module/global | Thread `ModuleId` through 117 mechanically matched sites. | `ModuleId` plus persisted `macro_bindings` relocation landed (#11033/#11084). PR #11191 classified the 12 named tables: qualified-path tables are lexical boundaries and global inference tables are inert. | **REFUTED (PR #11191):** the 117-site re-key was not a semantic migration target. The one real alias-order defect was #11176 and is fixed. |
| 2b — Struct | Add `StructId`, serialize it, and re-key 61 struct tables plus 20 bare gets. | `StructId`/`StructRegistry` re-key landed (#11156), moving `structinfo_name_maps_compile` 61→0. #11046 then centralized owner-scoped resolution and moved `struct_table_bare_gets_compile` 19→0 (#11413). | **complete with corrected architecture:** no `StructInfo` wire/relocation surface exists; Pattern A applies. Name-to-ID resolution is a single mutation-tested lexical boundary. |
| 3 — Function/method | Introduce `FunctionId`/`MethodId` and carry them through function values and method lookup. | Investigation found no bounded consumer that would retire a table; #11088 fixed the real owner-collapse symptom without unused IDs (#11098). | **moved to #11095:** retire identity-bearing function/method resolver sites structurally; do not introduce an unread ID. |
| cache lane | Persist relocation tables for every new ID. | Pattern B was proven by persisted `macro_bindings`; Pattern A covers rebuilt runtime contexts. | **per-field verdict:** require relocation only for fields that actually cross a wire format. |
| 4 — retirement | Drive every mechanical name-shaped count to zero. | The inventory now separates `identity-bearing`, `lexical-boundary`, and `inert` sites. | **re-scoped #10992:** only six-core-domain identity-bearing residuals are retirement targets; every downgrade requires landed evidence. |

Unadjudicated `other`-domain sites also fail closed to `identity-bearing` in
the inventory because the domain keyword scan has documented false negatives.
They remain outside #10459's six-domain Phase 4 total until individually
reclassified, but are never mislabeled inert.

## Phase sub-issues (epic tracking)

Filed as native sub-issues of #10459 (`gh issue create --parent 10459`); each
carries the `tech-debt` label, references #10459, and is seeded with this
document's per-phase numbers and acceptance criteria drawn from the epic
body.

| Phase | Issue | Scope |
|---|---|---|
| 1 (completion) | #10987 (**done**) | TypeVar projection identity residual: no rendered `String`/`Option<String>` participates in `runtime_typevar_projection_identities`'s key equality anymore; the key is the structural `TypeVarProjectionKey { owner: CoreType, binder_depth: usize, declared_lower: JuliaType, declared_upper: JuliaType }` (bounds parsed, not compared as strings — they must stay in the key because the body-derived owner does not encode them; `JuliaType` to keep module qualification; owner normalization preserves nested `UnionAll` binders), and the display name lives only on the stored `RuntimeTypeVarValue`. |
| 2a | #10988 (**foundation + persisted consumer landed**) | `ModuleId`, deterministic registration, and the Pattern A/B cache contract landed. The original 117-site re-key premise was **REFUTED by PR #11191** after per-table adjudication. |
| 2a continuation | #11032 (**closed by classification + one real bug fixed — see "Phase 2a continuation" below**) | Judge each of #10988's 12 re-scoped named tables individually: migrate to `ModuleId` only if doing so retires a real collision and lowers a baseline, else document why it legitimately stays name-keyed. Verdict: all 12 are either canonical-path-keyed (collision-safe by construction, mechanical-rename-only) or bare-name-keyed-but-verified-inert (widen-to-`Any`/dynamic-resolution safety nets) — except `module_aliases`, whose builder had a REAL same-bare-alias-different-owner bug (Issue #11176, fixed: source-order-driven first-using-wins instead of `HashSet`-iteration-order-dependent last-write-wins). Mirrors #10989/#10990's own "investigated, real bug fixed instead of the ID migration" shape. |
| 2b | #10989 (**historical investigation**) | The bounded-slice attempt declined an unused parallel ID and fixed #11021. Its “no `StructId`” and wire assumptions were later **superseded/refuted by PR #11156**. |
| 2b continuation | #11078 (**complete through #11046**) | `StructId`/`StructRegistry` key layouts; Pattern A proved no `StructInfo` relocation exists. #11046 retired all 19 bare resolver decisions and the associated compatibility fallback tables. |
| 3 | #10990 (**investigated; unused IDs declined**) | The investigation fixed #11088 and proved that introducing unread `FunctionId`/`MethodId` carriers would not retire an identity decision. |
| 3 continuation | #11095 | Owns the actual identity-bearing function/method resolver and table residual, including using-scope dispatch visibility; an ID is introduced only with a production consumer. |
| cache lane | #10991 | Pattern B is required only for identity-bearing fields that cross a wire; Pattern A covers rebuilt state. `macro_bindings` proves the persisted case and `StructRegistry` proves the derived case. |
| 4 | #10992 | Retire verdict=`identity-bearing` residuals, require evidence for every downgrade, then promote the stabilized inventory into an enforcement gate. |

## Phase 2a status (landed slice, Issue #10988)

The foundation landed: `ModuleId(u32)` + `ModuleInternTable`
(`subset_julia_vm_bytecode::module_intern`), allocated by module registration
order (a deterministic depth-first walk of the `Program`'s module tree,
`compile/collect.rs::register_module_ids`), plus the cache-relocation pattern
(`docs/vm/CACHE_ARCHITECTURE.md`'s "Owner-scoped id relocation pattern"
section — two sub-patterns depending on whether a table is genuinely
bincode-serialized or `RuntimeCompileContext`-adjacent/derived) and
same-name-different-module regression tests, both fresh compile and cache
restore.

**Historical #10988 scope note (resolved by PR #11191)**: of the 117
module/global sites this row originally scoped, this PR migrated the KEY TYPE of exactly
**one** table end-to-end (`CompiledProgram::macro_bindings`, `String` ->
`ModuleId`) — chosen over the 12 tables the issue body named
(`module_functions`/`module_exports`/`module_constants`/
`module_abstract_names`/`module_imported_bindings`/`module_struct_names`/
`module_aliases`/`module_usings`, `global_types`/`inference_global_types`/
`global_struct_names`/`global_const_structs`) because auditing the ACTUAL
(de)serialize code paths (not just lexical proximity to `compile/cache.rs`)
found none of those 12 are fields of any struct that is itself
bincode-serialized. Precisely: `module_functions`/`module_exports`/
`module_constants`/`module_struct_names`/`module_usings` are transient
`CorePipeline` fields (`compile/pipeline_ctx.rs`); `module_aliases` is a
`CoreCompiler` field (`compile/core_compiler.rs`); `module_imported_bindings`/
`global_types`/`inference_global_types`/`global_const_structs` are
`SharedCompileContext` fields (`compile/context.rs`); `global_struct_names`
is `CorePipeline`-local (`pending_global_struct_names`); and
`module_abstract_names` is never stored as a struct field at all — it is
threaded as a `&HashMap<String, HashSet<String>>` function parameter,
rebuilt locally on each call. All twelve are discarded once
`CorePipeline::finalize()` produces the `CompiledProgram` that is actually
serialized. The one exception is `inference_global_types`, which IS cloned
into `RuntimeCompileContext::inference_global_types` at `finalize()` time —
but `RuntimeCompileContext` itself is entirely `#[serde(skip)]` on
`CompiledProgram` (Issue #3973), so even that copy never crosses the wire.
None of the 12 therefore needed the persisted-relocation deliverable this
issue asked for. `macro_bindings` (not one of the 12 named tables, found
during the audit) is a REAL `CompiledProgram` field with `#[serde(default)]`,
so it is the one table in this domain that actually exercises a persisted
relocation (`CACHE_VERSION` 136 -> 137).

At the #10988 landing snapshot, the 12 named tables remained
`HashMap<String, _>`-keyed, each borrowed as
`&'a HashMap<String, HashSet<String>>`/similar through `CoreCompiler`/
`CorePipeline`/`SharedCompileContext` and consumed across 40+ files in
`subset_julia_vm_compile/src/compile/` (98 call sites for `module_functions` alone,
197 for `global_types`). Converting their key types is a blast radius
comparable to Phase 2b's own struct-table migration, not a small addendum —
re-scoped to a follow-up issue (linked from #10988) rather than force-fit into
one PR. The mechanical inventory (`docs/vm/SEMANTIC_ID_INVENTORY.tsv`,
regenerated in the landing PR) shows the resulting site-count delta: module
domain 54 -> 55 (net **+1**, the interning table's own internal
`HashMap<String, ModuleId>` — the single legitimate bare-name/ID resolution
boundary the target model's required property #3 calls for, which the
mechanical scanner correctly still flags as a `map_decl` shape even though it
is not debt) and one `other`-domain site retired in
`subset_julia_vm_bytecode/src/program.rs` (`macro_bindings`'s key type no
longer matches the scanner's `HashMap<String, _>` shape). `global` domain is
unchanged (0 of its 63 sites touched — `global_types`/`global_struct_names`
in `compile/cache.rs` are REPL-session-provided compile-time parameters, not
cache-serialized fields either).

## Phase 2a continuation (Issue #11032): none of the 12 named tables move a baseline

Issue #11032 picked up the 12 tables #10988 re-scoped (above) with the explicit
instruction to judge, per table, whether a `ModuleId`-keyed migration would
retire real name-lookup sites and lower a `check_name_based_lookup.sh`
baseline, or whether the table is a lexical-resolution boundary that
legitimately stays name-keyed (property #3) — migrating the former, only
documenting the latter, and treating "a migration that moves no baseline" as
the forbidden parallel path the #10989/#10990 precedent (below) twice upheld.

**Verdict: none of the 12 qualify for a `ModuleId` re-key.** Every one is
either (a) collision-safe **by construction** today, so a re-key would be a
pure representation swap with no bug retired, or (b) a genuine bare-name
accumulation that is **verified inert** by targeted MWEs against upstream
Julia 1.12.6, protected by an existing, independent safety net. There is also
no `check_name_based_lookup.sh` pattern for the `module`/`global` domains at
all (its six patterns only ever gated `typevar`/`struct`), so — unlike Phase
2b's `structinfo_name_maps_compile`/`struct_table_bare_gets_compile`, which
already existed as baselines #11078 could lower — there is no existing
baseline any of these 12 tables could lower even in principle without first
inventing a new gate for a domain that turns out not to need one.

| Table | Owner | Key shape | Verdict |
|---|---|---|---|
| `module_functions` | `CorePipeline` (`pipeline_ctx.rs`) | fully-qualified module path (`collect_module_info`'s `format!("{}.{}", prefix, name)`) | **stays name-keyed.** Byte-identical to the path `register_module_ids` interns as a `ModuleId` (pinned by the existing `register_module_ids_matches_collect_module_info_paths_issue_10988` test) — collision-free by construction: two distinct modules can never render the same fully-qualified path. Re-keying is a pure mechanical swap, not a bug fix. |
| `module_exports` | `CorePipeline` | same | same reasoning, same verdict |
| `module_constants` | `CorePipeline` | same | same reasoning, same verdict |
| `module_struct_names` | `CorePipeline` | same (`format!("{}.{}", prefix, name)` in `collect_module_structs`) | same reasoning, same verdict |
| `module_usings` | `CorePipeline` | same (`collect_module_usings`) | same reasoning, same verdict |
| `module_abstract_names` | function-parameter-threaded (never a struct field) | same (`collect_module_abstract_names`) | same reasoning, same verdict |
| `module_imported_bindings` | `SharedCompileContext` | compound `"{importing_qualified_name}.{symbol}"` key AND value (`resolve_module_imports`) | **stays name-keyed.** Both components are built from an already-qualified module path plus a dot-free Julia identifier, so the flattened compound string is injective (no two distinct `(module, symbol)` pairs can render the same compound string). A `(ModuleId, String)` re-key (the issue's own suggested shape) would be mechanical, not a bug fix. |
| `global_types` | `SharedCompileContext` | bare variable/global name, ACCUMULATED across every module body into one flat map (`resolve_global_types`, via `collect_global_types_for_inference`) | **verified inert.** Two sibling modules declaring a same-named module-level `const` with DIFFERENT types (`module A; const SHARED=10; end` / `module B; const SHARED=3.5; end`) do share one flat key — but `collect_global_types_for_inference_impl`'s merge explicitly widens to `ValueType::Any` when a re-inserted name's type disagrees with what is already recorded (the Issue #4285 safety net), so the type-directed compile decision degrades to fully dynamic instead of committing to the WRONG static type. Confirmed against upstream Julia 1.12.6: `M1.f()`/`M2.g()` referencing their own same-named bare constant both return the correct value and `typeof`. |
| `inference_global_types` | `SharedCompileContext`, cloned into `RuntimeCompileContext` at `finalize()` (per #10988's own finding, `#[serde(skip)]` — never crosses the wire) | same construction path as `global_types` (widened clone) | same reasoning, same verdict |
| `global_const_structs` | `SharedCompileContext` | bare variable/global name, accumulated across module bodies, gated to ZERO-ARGUMENT struct constructors only (`const M = SomeSingleton()`) | **verified inert.** Unlike `global_types` this insert has no widen-to-`Any` guard — it unconditionally overwrites — but MWE testing (two sibling modules each declaring a same-named zero-field singleton-struct constant, `A.M::FooSingleton` vs `B.M::BarSingleton`) shows `typeof`/`isa` resolve correctly for BOTH in sjulia, matching upstream: the actual field/type decision at the reference site is not blindly trusting this cache. |
| `global_struct_names` (`CorePipeline`-local `pending_global_struct_names`) | `CorePipeline` | bare variable name, REPL-session-provided | same bare-name shape as `global_types`; not independently exercised beyond the `global_types`/`global_const_structs` MWEs above (no distinct consumption path found), so classified the same way rather than re-tested — flagged here as an assumption, not a separately confirmed inertness finding |
| `module_aliases` | `CoreCompiler` (`core_compiler.rs`) | KEY: bare import-local alias name (e.g. `Sub` after `using .A: Sub`) — legitimately lexical, the property-#3-sanctioned kind of name-keyed boundary, since an alias's whole point is a local shorthand. VALUE: the resolved fully-qualified module path (same canonical-path shape as `module_functions` et al) | **stays name-keyed for the alias; VALUE re-key would be mechanical only** — BUT auditing this table surfaced a REAL bug, not a migration candidate: see below. |

### The one real bug found: Issue #11176 (`module_aliases`, fixed here)

Unlike the other 11 tables, `module_aliases`'s builder,
`imported_submodule_aliases` (`core_compiler.rs`), iterated an **unordered**
`usings: &HashSet<String>` and unconditionally overwrote a bare submodule
alias on every match. When two different `using` imports bring a same-named
submodule into scope from two different parent modules (`using .A: Sub` then
`using .B: Sub`, both `A` and `B` declaring their own `Sub`), upstream Julia
keeps the FIRST import and warns about the conflicting second one; sjulia
picked whichever module its `HashSet`/`HashMap` iteration visited last — an
incidental, not designed, "winner", and observably WRONG (`B.Sub` instead of
`A.Sub`) against upstream Julia 1.12.6. This is the same *shape* of bug
#10989/#10990 each found in their own domain (#11021 struct identity, #11088
function identity) — a same-spelling-different-owner collision the epic's
required property #3 exists to prevent — just in the module-alias-resolution
domain rather than struct/function identity.

Fixed by rebuilding `imported_submodule_aliases` to take the already-available
`resolved_usings: &[ResolvedUsingImport]` (source-ordered — both
`CoreCompiler::new`/`new_for_function` already construct it via
`resolve_scope_using_imports(&program.usings, ...)`/
`resolve_scope_using_imports(&module.usings, ...)`, themselves built from the
IR's declaration-ordered `Vec<UsingImport>`) instead of the unordered
`usings`/`imported_symbols` `HashSet`s, and using
`entry(name).or_insert(module_path)` (first-wins) instead of `insert` (whoever
is-last-in-iteration-order wins). No new type was needed — matching
#10989/#10990's own finding that the real bug in each domain did not require
the identity type the phase was investigating. Verified against upstream with
two independent scopes exercising BOTH orderings of the same conflict
(`subset_julia_vm/tests/fixtures/modules/submodule_alias_first_using_wins_11176.jl`),
proving the fix is driven by source order and not by name/hash order.

### Why this is the honest outcome, not an under-delivered one

`check_name_based_lookup.sh`'s six patterns cover `typevar`/`struct` only —
adding a *new* pattern for `module`/`global` and then "lowering" it within the
same PR that introduced it would not be a ratchet, it would be measuring a
gate against itself. The mechanical inventory
(`docs/vm/SEMANTIC_ID_INVENTORY.tsv`, regenerated) confirms zero `map_decl`
changes in the `module`/`global` domains from this issue's diff (module domain
stays at 55, global stays at 63 — the only inventory delta versus the
committed baseline is unrelated drift from other parallel work merged to
`origin/main` in the interim, reconciled the same way #10988's own landing
reconciled +2 unrelated sites). The 12 tables named by #10988's original scope
are conclusively classified, not merely deferred again: 11 are canonical-
identity-keyed or compound-canonical-keyed (no further Phase-2a-continuation
work needed on them), and the twelfth's real defect (#11176) is fixed. No
further "Phase 2a continuation 2" issue is filed for the named 12 — the
domain's remaining `map_decl`/`by_name_ref` mass captured by
`SEMANTIC_ID_INVENTORY.tsv`'s `module`/`global` rows was, at the #11191
landing snapshot, the 43 sites *outside*
the 12 named tables (formatting/diagnostics helpers, REPL delta bookkeeping,
etc.), which #10988's own scope never claimed and this issue was not asked to
re-audit. Those historical counts are not the current headline inventory;
regenerate the TSV and use the verdict totals above for current Phase 4 scope.

## Historical Phase 2b investigation (Issue #10989)

> **SUPERSEDED / PARTLY REFUTED by PR #11156:** this section records why
> #10989 declined an unused `StructId` at that point in time. The continuation
> later found a consuming re-key and landed `StructId`/`StructRegistry`.
> Its cache premise was refuted: the registry is Pattern A and no
> `StructInfo` relocation table exists. Current status is the as-landed table
> above and “Phase 2b landed slice” below.

Unlike Phase 2a, this phase found **no bounded, self-contained table**
analogous to `macro_bindings` — every candidate `StructId` consumer cascades
into the full 330-site `requires-owner-context-plumbing` bulk this document's
own headline table already flagged, so no `StructId` type was introduced.
Building one anyway (an `id: StructId` field added to `StructInfo`/
`StructDefInfo` that nothing reads for an identity decision, with
`struct_table`/`base_struct_table` left `HashMap<String, _>`-keyed) would
have been "a parallel `StructId` path alongside the tables" — the exact shape
this document's Phase 2b task list forbids — for ~55 risky construction-site
edits and a `CACHE_VERSION` bump with zero baseline movement. Confirmed via
advisor review before writing any such code.

**What #10989 landed instead**: a real, verified fix for Issue #11021 (same-
named structs in sibling modules wrongly comparing `==`/`===` as one type)
that needed no `StructId` at all. Root cause: four comparison functions
(`type_objects_equal`/`type_objects_identical` in
`subset_julia_vm_vm/src/vm/type_utils.rs`; `JuliaType::type_eq` and
`is_subtype_of_with_lookup`'s Struct-vs-Struct arm in
`subset_julia_vm_types/src/types/julia_type/{mod,comparison}.rs`) each
stripped BOTH sides' module-qualification prefix unconditionally before
comparing struct names — a fix originally added for Issue #8100 (a bare
in-module reference must `===` its own qualified name), over-applied to also
silently equate two DIFFERENT modules' same-named declarations. Fixed by
making the strip asymmetric: safe only when at most one side carries an
owner prefix; when both are qualified, the owners must match.
`type_objects_equal` additionally short-circuits to `false` on a known-owner
mismatch instead of falling through to the mutual-subtype fallback, which
routes through `CoreSubtypeEngine`/`CoreType` — and `CoreType::Struct`
construction (`subset_julia_vm_types/src/inference_core/type_core.rs`'s
`from_julia_name_uncached`, via `base_type_name`) discards module
qualification entirely at construction time, so that fallback cannot see two
different owners on its own. Verified against upstream Julia 1.12.6 with an
11-case identity matrix (bare/parametric structs, nested modules, Base-name-
shadowing structs) matching exactly for `==`, `===`, and `typeof` identity;
regression fixture `subset_julia_vm/tests/fixtures/modules/module_struct_identity_matrix_11021.jl`.

**`check_name_based_lookup.sh` baselines unchanged** (`structinfo_name_maps_compile`
61, `struct_table_bare_gets_compile` 20) — nothing in the struct-table family
was retired, so lowering either baseline here would have been dishonest
ratchet-gaming, not a real reduction in debt.

**Also found, filed separately, not fixed here**: Issue #11076, a pre-
existing method-dispatch ambiguity for same-named struct parameter types
from sibling modules (`f(x::A1x.Box) = ...` / `f(x::A2x.Box) = ...` resolves
ambiguous instead of picking the matching candidate) — the same structural
bug class as #11021 but in the dispatch-matching path (`comparison.rs`'s
`extract_type_bindings_with_lookup`/specificity ranking), which is
higher-blast-radius (changes which method a call resolves to, not just
whether two type values compare equal) and needs its own fix, not a copy of
#11021's four-site patch. Confirmed pre-existing on `origin/main` via an
isolated control worktree at the merge-base commit, not introduced by this
investigation's changes.

**Follow-up**: Issue #11078 (`techdebt(#10459): Phase 2b continuation`)
carries the full decomposition — the `struct_table`/`base_struct_table`
re-key (61+20 sites), `StructInfo`/`StructDefInfo` construction-site
migration (~55 sites, counted mechanically at filing time), `CoreType`
construction-time module-stripping fix (44+ `inference_core` sites), and
Issue #11076, with a suggested landing order (smallest blast radius first,
mirroring #11032's role for Phase 2a's own continuation).

## Phase 2b landed slice (Issue #11078): the struct tables are re-keyed

The re-key #10989 could not fit and #11078 decomposed **landed**. What moved:

| `check_name_based_lookup.sh` pattern | before | after |
|---|---:|---:|
| `structinfo_name_maps_compile` | 61 | **0** |
| `struct_table_bare_gets_compile` | 20 | 19 |

Inventory (`docs/vm/SEMANTIC_ID_INVENTORY.tsv`, regenerated): struct domain
353 -> 266; `anchor` kind 93 -> 34; grand total 874 -> 795.

`StructId { module: ModuleId, local: u32 }` + `StructRegistry`
(`subset_julia_vm_bytecode/src/struct_registry.rs`) replace
`SharedCompileContext::struct_table` / `RuntimeCompileContext::struct_table`.
Entries are keyed by the id; names are ALIASES into that id space through one
`name -> StructId` index (property #3's sanctioned lexical boundary).
`base_struct_table` is now `HashMap<String, StructId>` — an alias map into the
SAME id space, not a second table of layouts.

Three findings worth carrying forward, because they contradict what #11078's own
body assumed:

1. **The wire-format half was unnecessary.** `StructInfo` derives no
   `Serialize`, and `RuntimeCompileContext` is `#[serde(skip)]` (#3973): the
   struct table is REBUILT on both the fresh and cache-restore lanes. So the id
   is **derived, never persisted** (`CACHE_ARCHITECTURE.md` Pattern A) and needs
   no relocation table — the issue's "~25 `StructDefInfo` construction sites +
   `CACHE_VERSION` bump + relocation" scope does not exist. (`CACHE_VERSION` was
   bumped 142 -> 143 anyway, because a schema-fingerprint file changed.)

2. **`local` must NOT be a fresh per-module counter.** The two lanes register
   structs in genuinely different orders (the cached lane seeds every cached
   `struct_defs` entry, parametric instantiations included, up front; the fresh
   lane creates those much later, after the user's structs), so a registration
   counter allocates DIFFERENT ids on the two lanes and fails the parity
   requirement. `local` is therefore the existing dense concrete-type index
   (`StructInfo::type_id`), which the Base cache is already built to keep stable
   (#10265). The new information in a `StructId` is the OWNER. Determinism of
   that owner is a property of SEEDING the registry's module table from
   `register_module_ids`' walk before any struct is registered — done on both
   lanes, with the negative control pinned as a unit test.

3. **The semantic gain is "shadow, don't destroy".** A name-keyed table
   physically LOSES an entry when a module's bare alias overwrites a same-named
   one; that loss is the sole reason `base_struct_table` +
   `base_origin_bare_names` exist (#10078/#10257). Keyed by id, a colliding
   alias only re-points a name. This is the PRECONDITION for retiring that
   workaround — see residual (1) below.

### One interpretation divergence, stated so Phase 3 does not inherit it silently

#11078's body says parametric instantiations "must share the SAME `StructId`
since they're the same declaration" (`Box{Int64}` and `Box{Float64}` being one
declaration). The landed `StructId` does the OPPOSITE: it is per **concrete
DataType** (`local == type_id`), so those two get distinct ids — which is also
what upstream Julia's own model says (`Box{Int}` and `Box{Float64}` ARE different
`DataType`s; what they share is a `TypeName`).

This is *moot* for the work here — per-entry identity is exactly what re-keying a
table of per-instantiation `StructInfo` layouts needs, and Pattern A removed the
`StructDefInfo.id` field where declaration-level identity would have mattered —
but it is a real semantic choice, not an oversight. If a future phase needs
DECLARATION identity (a `TypeName`-like id shared by every instantiation of a
family), that is a SECOND id, layered on the parametric-family tables
(`parametric_structs`), not a redefinition of this one. The Phase 3
(`FunctionId`/`MethodId`, #11095) author should not assume declaration-level
semantics from this precedent.

### Known limitation of the landed `StructId`: the owner can be `Main` when it should not be

Surfaced by an adversarial `codex` review of the landing PR, and stated here rather
than left for someone to rediscover:

`StructRegistry::insert` derives the owner from the FIRST name a `type_id` is
registered under. For a parametric instantiation that the pipeline pre-instantiates
under its BARE spelling (`Box{Int64}`) before the qualified one (`M.Box{Int64}`) is
seen, that first name has no module prefix, so the entry's `StructId::module` is
`Main` even though `M` declares it.

This is **inert today**: no dispatch, subtype, or codegen decision reads
`StructId::module` — the ids exist to key the table and the names still resolve
exactly as before (the qualified and bare spellings alias one entry, with one
layout). It is NOT inert for the residual work below: the moment an owner/scope-aware
resolver (or anything else) starts making a decision from `StructId::module`, this
must be fixed first, by threading the DECLARING module into `insert` instead of
inferring it from the name. Two nearby bugs make the same point from the other side:
**#11167** (a module struct whose bare name collides with a Base parametric family
overwrites that family's `struct_defs` row — the registry now contains, but does not
fix, the resulting one-`type_id`-two-declarations case) and **#11153** (a module
struct named `Dict` routes its constructor to Base's `Dict()`).

**Resolved by Issue #11046:** `insert_owned` now carries the declaring module
separately from the display spelling. The scoped resolver was introduced only
after the module-owned bare-parametric case was pinned by a regression test.

### Historical residual after the re-key (resolved by #11046)

- **`struct_table_bare_gets_compile` was 19.** Retiring these required an
  owner/scope-aware resolver that changes WHICH struct a bare name resolves to
  (today the last-registered module wins the bare slot). That is a semantic
  change with Base-wide blast radius, not a type swap; it must not ride along
  with a mechanical re-key. It also subsumes retiring
  `base_struct_table`/`base_origin_bare_names` and the `lookup_bare_struct_info`
  / `julia_type_to_value_type_with_origin_table` / `prefer_base_origin` anchors
  that the earlier `check_name_based_lookup.sh` required to exist. #11046
  replaced them with `StructRegistry::resolve_scoped`; the count is now zero.
- **`CoreType::Struct` owner-awareness is largely OBE.** #11078's item 3 (and
  its "44 `inference_core` sites") predates PR #11138, which made the DISPATCH
  projection owner-preserving (`CoreType::from_julia_name_for_dispatch` /
  `preserve_user_owner`, gated by `has_qualified_nominal_family_collision`) and
  thereby fixed #11076 AND #11094. What remains is only the NON-dispatch
  consumers (`typejoin`, promotion, specificity ranking), a much smaller and
  differently-shaped job than the issue's count implies.
- **`StructDefInfo.id`**: unnecessary, per finding 1.

### Phase 2c landed slice (Issue #11046): owner-scoped resolution

`StructRegistry` now owns a declaration-only `(ModuleId, local spelling) ->
StructId` index in addition to its lexical alias map. `resolve_scoped` applies
exact-qualified, current-module, Main/Base-origin, then lexical-alias ordering.
This made the old `base_struct_table`/`base_origin_bare_names` recovery path
redundant; canonical entry enumeration also exposes shadowed field layouts
without a second table.

The `struct_table_bare_gets_compile` ratchet moved 19 -> **0**. The registered
negative control disconnects the Main-owner resolver branch while preserving
the surrounding API, proving the audit checks delegation rather than token
co-occurrence. The remaining inference `HashMap<String, StructTypeInfo>` is
explicitly a lexical field-layout projection behind `lookup_struct_type_info`;
`type_id`/`ConcreteType::Struct` carry semantic identity.

### Gate-ownership note (Issue #11078)

`check_name_based_lookup.sh` — the ratchet this whole epic is measured by — was
never registered in `scripts/source_only_audits.tsv`, so neither
`run_source_only_audits.sh` nor `premerge_gate.sh` ever ran it, and it sat RED
on `origin/main` (`typevar_core_bindings` drifting 12 -> 13 -> 15 across PRs
#11096/#11138) while the aggregate audit reported green. Registered and
reconciled in #11078. Same class as #10870. Note also that
`check_audit_negative_selftest.sh`'s `run_selftest` DOWNGRADES a red clean tree
to a NOTE rather than failing, which is why the drift stayed invisible there too.

## Phase 3 as-landed judgment (Issue #10990; continuation #11095)

> **CURRENT VERDICT:** no `FunctionId`/`MethodId` was introduced because no
> bounded production consumer would retire a table. This is not a deferral of
> an ID-shaped deliverable: #11095 owns the identity-bearing resolver/table
> decisions, and may introduce an ID only where production reads it.

Same conclusion as Phase 2b, independently re-derived for the function/
method-sig domain: **no bounded, self-contained table** analogous to
`macro_bindings` exists. `SharedCompileContext::function_indices: HashMap<String,
usize>` and `SharedCompileContext::source_ordered_method_sigs: HashMap<String,
Vec<SourceOrderedMethodSig>>` (`subset_julia_vm_compile/src/compile/context.rs`),
`CorePipeline::method_tables: HashMap<String, MethodTable>`
(`subset_julia_vm_compile/src/compile/pipeline_ctx.rs`), and `imported_functions:
HashSet<String>` (same file) are all `CorePipeline`/`SharedCompileContext`-
transient compile-pipeline state — none is a field of any struct that is
itself bincode-serialized, the same reasoning #10988 used to exclude 12 of
its own named module/global tables and #10989 used to decline `StructId`.
The one genuinely `CompiledProgram`-serialized function-domain field,
`functions: Vec<std::rc::Rc<FunctionInfo>>`
(`subset_julia_vm_bytecode/src/program.rs`), is **already index-keyed**
(`global_index` into the `Vec`, used pervasively as the call target
throughout the VM) — a `FunctionId` there would formalize an owner
derivation already computed once at registration
(`format!("{}.{}", module_path, func.name)`, baked into `FunctionInfo.name`
today), not retire a bare-name `HashMap`. Building `FunctionId`/`MethodId`
against the transient tables anyway (an id nobody reads, with
`function_indices`/`method_tables` left `String`-keyed) would be the same
"parallel path" #10989's advisor review rejected for `StructId`, so no
`FunctionId`/`MethodId` type was introduced.

**What #10990 landed instead**: a real, verified fix for Issue #11088 (same-
named functions in sibling modules wrongly comparing `==`/`===` and sharing
a `typeof`) that needed no `FunctionId` at all. Root cause:
`emit_function_value_named` (`subset_julia_vm_compile/src/compile/core_compiler.rs`)
always baked the bare declared name into a resolved function value's runtime
type identity — a correct fix for Issue #10077 (the SAME declaration must
report the SAME `typeof` regardless of qualified-vs-bare/imported access),
over-applied: it did not distinguish that invariant from two DIFFERENT
declarations that merely share a bare name across sibling modules. Fixed by
checking, when a qualified access's identity name is chosen, whether ANOTHER
module's qualified `method_tables` key also ends in the same bare name —
deliberately a KEY-existence check across `method_tables`' keys, not a
candidate-set comparison against the shared bare-name table's method LIST,
because `MethodTable::add_method`'s same-signature dedup (Issue #8079) can
silently evict one sibling's entry from that list in favor of the other's,
which would give an asymmetric (compile-order-dependent) answer instead of a
symmetric one.

A pure key-existence check over-fires, though: it also flags an unrelated
module that shares a bare name but was never `using`d, wrongly diverging a
genuinely `using`d declaration's bare-vs-qualified identity apart (a
regression of Issue #10077's own invariant, caught by adversarial review
before landing). Fixed by resolving each access path's owning module
symmetrically via a new `unique_using_owner` helper — the single `using`d
module (if exactly one) that actually brings a bare name into scope — and
only treating two declarations as distinct when the bare name does NOT
uniquely resolve back to the same owner through a real `using`. A second
adversarial pass then found `unique_using_owner` itself used
`module_functions` (everything a module *defines*) instead of
`module_exports` (what it actually *exports*): upstream `using M` only
brings `M`'s exported names into scope, so a `using`d module that merely
*defines* the same bare name privately is not a real candidate owner. Fixed
by gating `unique_using_owner` on `module_exports` too, reusing the same
export-visibility rule the file's existing `imported_submodule_aliases`
helper already applies to the identical question.

Verified against upstream Julia 1.12.6 with an 18-assertion identity matrix
(sibling-module collision both directions, nested-module collision,
self-identity, calling correctness, the Issue #10077
bare-vs-qualified-same-declaration invariant, and both adversarial-review
regression guards) matching exactly; regression fixture
`subset_julia_vm/tests/fixtures/dispatch/module_function_identity_matrix_11088.jl`.
This composes correctly with Issue #11021's already-landed owner-aware
struct comparison fix — `typeof(f1) === typeof(f2)` for two colliding
function declarations (not just the direct value `f1 === f2`) now correctly
returns `false` end-to-end, confirmed by rebuilding against `origin/main`
post-#11021 rather than the stale pre-#11021 worktree state this
investigation started from.

**`check_name_based_lookup.sh` baselines unchanged** — none of its six
patterns cover the `function`/`method-sig` domains (they only ever gated
`typevar`/`struct`), and nothing in the function-table family was retired,
so there is no baseline to lower here.

**Also found, filed separately, not fixed here**: Issue #11089, a pre-
existing bare-name method-table visibility leak — `using .M1x` (bringing
exactly one module into scope) does not scope which module's methods are
consulted at an unqualified call site, because `imported_functions` is a
flat "was this name imported from anywhere" set and every module-scoped
function is registered into the shared bare-name `method_tables` entry
unconditionally, regardless of any actual `using`. A module that is NEVER
`using`'d anywhere in the program can still win dispatch over the one
module actually in scope. Higher blast radius than #11088 (changes which
method a call resolves to, not just whether two function values compare
equal), needs per-scope module-visibility tracking that does not exist
today, and its fix site (`compile/expr/call/dispatch.rs`) is concurrently
being edited for the related-but-distinct Issue #11076 — deferred, mirroring
how #10989 deferred #11076 itself rather than fixing it in the same PR.

**Follow-up**: Issue #11095 (`techdebt(#10459): Phase 3 continuation`)
carries the identity-bearing `function_indices`/
`source_ordered_method_sigs`/`method_tables` resolver work and Issue #11089.
Its original `FunctionInfo.id` proposal is not independently required: the
as-landed verdict above permits an ID only when a production consumer reads
it and it retires a semantic decision.

## Caveats found while classifying (documented so they are not re-discovered)

- **Comment/string masking matters more here than in the panic-debt
  classifier**: this codebase's comments frequently spell out exact type
  signatures in prose (e.g. `` `HashMap<String, HashSet<String>>` `` inside a
  `//!` doc comment describing a serialization format). Unlike
  `panic_debt_classification.py` (which accepts this as a documented,
  unfixed caveat for its much shorter `.unwrap(`/`.expect(`/`panic!(`
  tokens), this script masks comments and string literals *before* scanning
  for `HashMap`/`BTreeMap` declarations — the false-positive rate for a
  multi-word generic-type token is high enough that leaving it unmasked
  would have measurably inflated the count (caught during authoring: an
  early unmasked version scored 882 sites, several of them comment mentions;
  the masked version scores 873, and a spot check confirmed the removed
  9 were exclusively comment text).
- **`_nearest_identifier`'s wrapper-peeling loop** (see the function's own
  docstring) handles `std::collections::HashMap<...>`,
  `Lazy<RwLock<HashMap<...>>>`, `-> HashMap<...>` return types,
  `-> (HashMap<...>, ...)` tuple return types, `impl Trait for
  HashMap<...>`, and `Variant(HashMap<...>)` tuple-enum-variant shapes — all
  found as real shapes in this codebase during authoring, not hypothetical.
  11/872 sites (~1.3%) still fall back to the literal `HashMap`/`BTreeMap`
  token because their declaration spans multiple physical lines (see "Known
  limitations" in the script).
- **`other`-domain false negatives are expected, not a bug**: the
  fixed-order keyword scan is a substring match, not semantic analysis (e.g.
  `subset_julia_vm/src/aot/analyze/core_ir_analyzer.rs`'s
  `call_graph: HashMap<String, HashSet<String>>`, a function-name-keyed call
  graph, scores `other` because neither `call_graph` nor `HashSet<String>`
  contains a domain keyword — arguably a `function`-domain site on manual
  inspection). Phase 2a/3 authors should not treat the `other` bucket as
  "confirmed out of scope", only as "not mechanically confirmed in scope".
