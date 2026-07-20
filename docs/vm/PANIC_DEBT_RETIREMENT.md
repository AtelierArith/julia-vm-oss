# Panic Debt Retirement Plan (Issue #10869)

Issue #10869 is the owner epic for retiring **grandfathered** panic sources
(`.unwrap()` / `.expect()` / `panic!()`) across every user-input-reachable
production path — as opposed to `docs/vm/PANIC_FREE.md`'s existing
enforcement infrastructure (Issue #8686/#8705-#8707), which stops new panic
sources from being added but does not itself pay down the pre-existing
baseline. This document is the **Phase 0** deliverable: classify the
existing debt, rank it by entrypoint proximity, and hand off Phases 1-3 to
tracked sub-issues.

## Scope disclaimer — read this before drawing conclusions from the numbers

This document (and `scripts/panic_debt_classification.sh`) measures exactly
three static Rust source patterns: `.unwrap(`, `.expect(`, `panic!(` — the
same three the Issue #10869 evidence table and
`docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv` track. **This is not the same as
"cannot panic on malformed input."** The Issue #8705 inventory found
`clippy::indexing_slicing` to be the *largest* category by far — 7,717 hits
workspace-wide, vs. 1,144 `unwrap_used` and 1,190 `expect_used` at that time
— and this document says nothing about it, nor about `unreachable!()`,
arithmetic overflow, or native (Rust) stack overflow (see
`docs/vm/PANIC_FREE.md`'s "Bounding Host Recursion" section). A module shown
below with **zero** real production hits for these three tokens is "ready to
gain a `#![deny(clippy::unwrap_used, clippy::expect_used)]` pragma with
little or no code change" — it is **not** "already panic-safe." Phase 3's
malformed-input / fuzz corpora (process survival + typed error, not token
counts) remain the actual safety gate; nothing here substitutes for that.

## How to regenerate

```bash
bash scripts/panic_debt_classification.sh
```

This is a **report generator, never a gate** — it always exits 0, is not
wired into `premerge_gate.sh` or any `check_*.sh`, and simply rewrites
`docs/vm/PANIC_DEBT_CLASSIFICATION.tsv` plus a stdout summary (grand totals,
reconciliation against the ratchet baseline, top user-input-reachable
modules, and a `mask_non_code()` self-diagnostic). See the script's module
docstring for the full classification mechanism (path rules, cross-file
`#[cfg(test)] mod X;` closure, inline brace-scope detection) and its
`RULES_BY_FILE` table for the audited build-time-invariant /
cache-corruption-boundary file list.

## Headline counts (2026-07-13, this branch, reconciled against origin/main)

| bucket | unwrap_call | expect_call | panic_macro |
|---|---:|---:|---:|
| test-only | 1,062 | 583 | 224 |
| build-time-invariant | 11 | 9 | 28 |
| cache-corruption-boundary | 0 | 2 | 0 |
| **user-input-reachable** | **145** | **146** | **4** |
| **total** | **1,218** | **740** | **256** |

Reconciliation: `expect_call` (740) and `panic_macro` (256) match the Issue
#10869 evidence snapshot (origin/main `92a77484`) exactly; `unwrap_call`
drifted by +3 (1,215 → 1,218), which is expected — ordinary commits land on
`main` between the issue's evidence snapshot and any later classification
run (Issue #10870 resynced the ratchet baseline the same day the evidence
was recorded). Per-(metric, module) sums also match every row of
`docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv` exactly (83/83), since the
classification script scopes to the identical file set and copies
`module_key()` verbatim from `scripts/check_panic_free_ratchet.sh`.

**83% of all matched sites are test-only** (via cross-file `#[cfg(test)] mod
X;` closure or inline `#[cfg(test)]`/`#[test]` brace-scope detection — not
just the `_tests.rs`/`/tests/`/`/benches/` path patterns the existing
`panic_free_inventory.py` already used). This matters concretely: e.g.
`subset_julia_vm_vm/src/vm/mod.rs` declares `#[cfg(test)] mod tests;`, so
`vm/tests.rs`'s ~87 `.unwrap()` + 32 `.expect()` sites are test-only even
though the filename does not end in `_tests.rs`. Three named Phase-2 modules
— `vm/specialize`, `vm/type_ops`, `register_vm.rs` — turned out to have
**zero** real production hits for these three tokens; every one of their
raw `PANIC_FREE_RATCHET_BASELINE.tsv` counts comes from a trailing
`#[cfg(test)] mod tests { ... }` block (verified line-by-line, e.g.
`vm/specialize/mod.rs`'s `.expect`/`panic!` sites are 100% at or after line
505's `mod tests {`, none before). Per the scope disclaimer above, this
means those three modules are cheap to `#![deny(...)]`, not that Phase 2 is
done for them (`indexing_slicing` and IR-boundary work remain).

## Entrypoint-priority ranking

Ranked by proximity to the entrypoints Issue #10869 names (CLI, REPL, FFI,
Web, file compile, macro expansion, cache load) — every one of these
entrypoints funnels through parser → lowering → compile before reaching the
VM, so upstream stages rank higher regardless of their absolute token count.

| Rank | Module | Real user-input-reachable hits (unwrap+expect+panic) | Why this rank |
|---|---|---:|---|
| 1 | `subset_julia_vm_parser/src/parser` | 207 | First thing every entrypoint (CLI file/`-e`, REPL per line, file compile, FFI/Web compile) does with raw user bytes. Also the largest real concentration found — see caveat below, but the `.unwrap()` share (105) is genuinely almost all real production risk (no inline test noise: `grep -rl cfg(test)` on this directory returns nothing). Issue #10869 Phase 1a. |
| 2 | `subset_julia_vm_lowering/src/lowering` | 21 | Second pipeline stage, still on every entrypoint's hot path (CST → Core IR). Issue #10869 Phase 1b. |
| 3 | `subset_julia_vm_compile/src/compile` (excl. cache files, see below) | 21 | Third stage (Core IR → bytecode / inference), still per-entrypoint. Issue #10869 Phase 1b. |
| 4 | `subset_julia_vm/src/macro_runtime.rs` | 9 | Issue #10869 names macro expansion as its own entrypoint category, distinct from lowering/compile. |
| 5 | `subset_julia_vm/src/repl` (session/converters/globals) | 1 | Stateful — a bug here has session-lifetime blast radius (repeated hits per REPL process) even though Phase 0 found only one real site. Issue #10869 Phase 1c. Cheap near-term win: declare deny-modules once `session.rs:550` is closed. |
| 6 | cache load / deserialize boundary (`compile/cache.rs`, `embedded_cache.rs`, `preload_cache.rs`, `seeded_cache.rs`, `precompile.rs`, `core_ir_file.rs`, `vm_bytecode_file.rs`, `loader.rs`, `subset_julia_vm_ffi/src/bytecode.rs`) | 0 unwrap / 2 expect (both inside a `//!` doc-comment example, not real code — see caveats) / 0 panic | Named as its own entrypoint category (Issue #10869 Phase 1c). Already effectively clean for these three tokens; `deserialize_base_cache`/`.sjvmbc`/`.sjir`/`.ji.json` loaders already return `Result` and treat load failure as a cache miss (`docs/vm/CACHE_ARCHITECTURE.md`). Cheapest near-term deny-module win in the whole epic. |
| 7 | `subset_julia_vm_vm/src/vm` (top-level files) + `vm/exec/mod.rs` | 3 | `vm/exec/mod.rs`'s one `expect_call` is the pre-existing, documented `SystemTime::now()` exception (`docs/vm/PANIC_FREE.md` "Exceptions"), so 2 genuinely new sites. |
| 8 | `vm/specialize`, `vm/type_ops`, `vm/formatting`, `register_vm.rs` | 0 | Named in Issue #10869 Phase 2, but zero real hits for these 3 tokens today (see headline counts above) — Phase 2's remaining work here is `indexing_slicing` / IR-boundary typing, not unwrap/expect/panic. |
| 9 | `subset_julia_vm/src/aot` (incl. `codegen/cranelift`) | 22 | First-class backend (`docs/vm/ADR_BACKEND_STRATEGY.md`) reachable via the `juliars`/`bundle` CLIs and Issue #10869's "file compile" category, but gated behind the opt-in `aot`/`cranelift` Cargo features (not built in the default iOS/WASM profile) — lower default exposure than 1-6. 12 of these hits are inside Rust string literals that are Julia-runtime **codegen templates** (e.g. `"{}.pop().expect(\"pop! from empty collection\")"` — text the AoT backend emits into the *generated* Rust program, not a live call in `aot_codegen.rs` itself); see caveats. |
| 10 | `subset_julia_vm_ffi/src` (excl. `bytecode.rs`, ranked in row 6) | 0 | Already at zero for these 3 tokens; combined with the existing `catch_unwind` containment (Issue #8707) this is in the best shape of any entrypoint today. Containment is a backstop, not a substitute for 1-4 actually not panicking. |
| 11 | `subset_julia_vm_web/src/lib.rs` | 0 | Zero matches of any kind (not even test-only) in this file. All real Web-entrypoint risk is inherited transitively from parser/lowering/compile/VM (rows 1-4, 7). |
| — | `subset_julia_vm/src/base_loader.rs`, `stdlib_loader.rs` | (not user-input-reachable; see script's `RULES_BY_FILE`) | Parse the bundled, trusted Base/stdlib Julia source at startup — same trust class as `build.rs`, not an externally-reachable input. Judgment call, documented in the classification script. |

## Remediation-kind by module (sampled, not exhaustive)

Per Issue #10869 Phase 0's third checklist item — is the dominant fix a
**proof-backed invariant** (convert the existing panic to a `match`/typed
internal error per `docs/vm/PANIC_FREE.md`'s "Guarded Unwraps" pattern,
because the surrounding code already establishes the precondition) or a
**typed error conversion required** (the value can genuinely come from
malformed user input and there is no established precondition)?

### Parser (`subset_julia_vm_parser/src/parser`) — dominant: proof-backed invariant

- 101 of the 102 `expect_call` matches are **not** real `Option`/`Result`
  panics at all: they are calls to `Parser::expect(&mut self, expected:
  Token) -> ParseResult<SpannedToken<'a>>` (`subset_julia_vm_parser/src/parser/mod.rs:336`),
  an already-`Result`-returning checked helper invoked as `self.expect(Token::RParen)?`
  (e.g. `collections.rs:35,152,161,213`). The static token regex cannot
  distinguish a method literally named `expect` from `Option::expect` — this
  is a measurement artifact, not real debt.
- The dominant real pattern (>100 of the 105 `unwrap_call` sites) is
  `self.advance().unwrap()` immediately after a `match self.peek() { Some(Token::X)
  => ... }` arm that already proved a token is present (e.g.
  `expressions/primary.rs:105,115,123,189,217`; `parser/mod.rs:231,338`).
  Proof-backed today by control-flow discipline, not by the type system —
  exactly the "Guarded Unwraps" pattern `docs/vm/PANIC_FREE.md` prescribes
  converting to an explicit `match` with an internal-error arm.
- One genuine outlier needing a closer look: `collections.rs:843`,
  `rows.last_mut().expect("row continuation requires a preceding matrix row")`
  — proof-backed by caller discipline (only called when `continuation` was
  already established), but relies on the caller rather than the type,
  making it the closest thing to a "typed error conversion required"
  candidate in this module.

### Lowering / compile front door — dominant: proof-backed invariant (mostly self-documented)

Nearly every real hit in `subset_julia_vm_compile/src/compile` already carries an
explanatory message naming the invariant, e.g.:
`compile/abstract_interp/engine/mod.rs:1637` (`"checked active specialization above"`),
`compile/abstract_interp/worklist.rs:274` (`"entry block was allocated before lowering"`),
`compile/expr/builtin_array.rs:803` (`"guarded typed array element type"`),
`compile/expr/call/constructors.rs:777` (`"checked above"`),
`compile/ssa_ir/build.rs:513` (`"loop frame pushed above (Issue #8550)"`),
`compile/ssa_ir/plan.rs:586` (`"numeric_convert_target checked arity"`),
`compile/stmt.rs:1027,2384,2495,2618,2691,3715` (`self.loop_stack.pop().unwrap()`,
repeated six times — same invariant, one recurring shape).
In `subset_julia_vm_lowering/src/lowering`, the pattern is the same family — `.next()`/`.pop()`/`.last()`
on a `Vec`/`Option` whose non-emptiness was just established
(`lowering/expr/collection.rs:251,365,367,385,1758`;
`lowering/stmt/assignment.rs:1414,1503,1528` — literally `"checked is_some"` /
`"operator was pre-checked"`). These are the textbook "proof-backed invariant"
case: safe today by construction, but not type-enforced, so a future refactor
could silently violate the assumption — Phase 1b's job is exactly to convert
these into the guarded-match form.
One exception flagged for closer Phase 1b review rather than asserted safe:
`lowering/mod.rs:1215,2650,2685` (`self.parsed_rust.as_ref().unwrap()`) —
whether this Option is provably populated on every reachable call path was
not verified during Phase 0 and should not be assumed proof-backed without
checking.

**Status: DONE (Issue #10905).** All of the above converted to the guarded
form; the flagged `parsed_rust` sites were confirmed structurally safe (the
field is set unconditionally on the line immediately above each read, under
an exclusive `&mut self` borrow with no intervening re-entrant call) and
converted anyway, matching the "Guarded Unwraps" pattern rather than being
left as bare unwraps. See `docs/vm/DONE.md`'s 2026-07-14 entry and the Phase
sub-issues table below for the full per-site breakdown, the new
`internal_lowering_error`/`internal_compile_error` helpers, and the
`lowering_compile_malformed_input_10905_tests` fuzz corpus.

### REPL / session — proof-backed invariant, single real site

`repl/session.rs:550`: `.expect("delta_eligible implies a prior persistent
compile")`. The message names the invariant explicitly; Phase 1c's job is to
confirm it structurally (not just by comment) and convert to a typed
internal error, then declare the module deny-clean — the rest of
`repl/converters.rs`, `repl/globals.rs`, and the bulk of `repl/session.rs`'s
raw token count is inside `#[cfg(test)]` blocks (confirmed line-by-line).

**Status: DONE (Issue #10906).** Confirmed structurally — `delta_eligible`
(computed once, a few lines above the site) short-circuits on
`self.persistent_compile.is_some()`, and `try_live_delta_run` (the only
function called in between) never clears `persistent_compile` on any of its
`None`-returning paths — then converted to a guarded `match` returning
`REPLResult::error(...)`, matching this function's existing error-handling
idiom (`return REPLResult::error(...)` is already used for every other
compile-error path in `eval`). `repl/session.rs`, `repl/converters.rs`, and
`repl/globals.rs` all gained
`#![deny(clippy::unwrap_used)]`/`#![deny(clippy::expect_used)]` and are
registered in `docs/vm/PANIC_FREE_DENY_MODULES.tsv`. Regression coverage:
`subset_julia_vm/tests/regression_scope_session_tests.rs`'s
`repl_session_malformed_redefinition_10906_tests` module (malformed
multi-line input, empty input, embedded control characters, function/struct
redefinition sequences — 5 tests, all asserting typed `REPLResult` +
session survival, never a panic).

### Cache deserialize / load boundary — no real sites found

Zero `unwrap_call`, zero `panic_macro`, and both `expect_call` hits
(`core_ir_file.rs:34,37`) are inside a `//!` doc-comment usage example
(`//! // core_ir_file::save(&program, "output.sjir").expect("Failed to save");`),
not executable code. `deserialize_base_cache` (`compile/precompile.rs:728`)
and the `.sjvmbc`/`.ji.json` loaders already return `Result`/treat failure as
a cache miss per `docs/vm/CACHE_ARCHITECTURE.md`. Recommended Phase 1c action
for this bucket: declare the deny-module pragma (no code change expected),
then let Phase 3's malformed-cache corpus be the actual verification that a
truncated/foreign-version `.sjvmbc`/`.sjir`/base-cache blob cannot panic
through some path this token scan cannot see (e.g. `indexing_slicing` while
parsing the binary header).

**Status: DONE (Issue #10906).** `compile/cache.rs`, `compile/embedded_cache.rs`,
`compile/preload_cache.rs`, `compile/seeded_cache.rs`, `compile/precompile.rs`,
`core_ir_file.rs`, `vm_bytecode_file.rs`, `loader.rs`, and
`subset_julia_vm_ffi/src/bytecode.rs` (9 files, no production-code change
needed, confirming the "little/no code change expected" prediction above)
all gained the deny pragma and a `docs/vm/PANIC_FREE_DENY_MODULES.tsv` entry.
A first slice of the malformed-cache corpus this section anticipated for
Phase 3 was pulled forward and validated per-boundary here rather than
deferred: `vm_bytecode_file.rs` (bit-flipped `.sjvmbc` payload past an
otherwise-valid header, wrapped in `catch_unwind` to hard-guarantee no
panic), `core_ir_file.rs` (truncated-length and bit-flipped `.sjir` payload),
and `loader.rs` (malformed/truncated `.ji.json`, confirming `read_cache`'s
`.ok()?` chain collapses every failure to a cache-miss `None`) — 4 tests
total, none of which needed a code change (every load path already returned
`Result`/`Option`). The broader fuzz-style corpus across all four formats
together remains Phase 3's job (Issue #10908).

### Runtime/optimization paths (`vm/specialize`, `vm/type_ops`, `vm/formatting`, `register_vm.rs`, AoT) — Phase 2

**Status: DONE (Issue #10907).** Confirmed the Phase 0 prediction for the
four named VM modules: every one of their raw
`PANIC_FREE_RATCHET_BASELINE.tsv` counts was inline `#[cfg(test)]` code (both
whole-file `mod tests { ... }` blocks and, in `vm/specialize/stmt.rs`, a
single `#[cfg(test)] fn` with zero unwrap/expect calls in its body). All 15
files under `subset_julia_vm_vm/src/vm/specialize/`, `subset_julia_vm_vm/src/vm/type_ops/`,
`subset_julia_vm_vm/src/vm/formatting/`, and `subset_julia_vm_vm/src/register_vm.rs`
gained `#![deny(clippy::unwrap_used)]`/`#![deny(clippy::expect_used)]` plus a
`docs/vm/PANIC_FREE_DENY_MODULES.tsv` entry, with `#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]` on each real test module
(matching the Phase 1c precedent) — verified with workspace
`cargo clippy --all-targets -- -D warnings` (not `--lib`, per the sibling-PR
lesson that `--all-targets` is what actually pulls `#[cfg(test)]` code into
scope).

`subset_julia_vm_vm/src/vm` (top-level files) had exactly 2 real
`unwrap_call` hits, not the 0 the four named modules had: `vm/executable.rs`'s
optimizer typed-loop predecoder (`Instr::ReturnStruct if
pending_struct_return.is_some() => { ...take().unwrap() }`), converted to
`pending_struct_return.take()?` (this function already returns
`Option<TypedLoopBlock>` and uses `return None` for every other "can't build
a typed loop, fall back to the interpreter" rejection — zero behavior
change, same fallback path, no new error type needed). The second hit
(`vm/stack_ops.rs:29`) is inside a rustdoc `/// ``` ... ``` ` example, not
compiled code reachable by `cargo clippy` (`--all-targets` does not include
doctests) — left as-is, consistent with the "doc-comment examples" caveat
below.

`subset_julia_vm/src/aot`'s 22 real sites: 16 converted, 6 deferred to a
follow-up issue as a different remediation kind (see below).

- `aot/analyze/ir_converter/expr.rs` (2 `expect_call`, proof-backed
  `iter.next()`/`.into_iter().next()` guarded by a length check a few lines
  above) → `match`-converted to `AotError::InternalError`, matching this
  function's existing `AotResult` error style.
- `aot/specialization.rs` (3 `expect_call`, `self.instances.get_mut(key)`
  immediately after `self.enqueue(key.clone())`) → replaced with a private
  `ensure_enqueued()` helper built on `HashMap::entry()`, which returns
  `&mut CodeInstance` with **no** `Option` in its return type at all — a
  genuine "move to a newtype/validated IR boundary" fix per this issue's
  third acceptance criterion, not just a panic-to-typed-error swap.
- `aot/types.rs` (2 `expect_call`, a match-guard `.is_some()` check
  re-derived a second time in the arm body) → merged the two guarded arms
  into one `if let (Some(_), bool) = (...)` / `else if let Some(_) = ...`
  chain that binds each `Option` exactly once — again a real elimination,
  not a stand-in error.
- `aot/mod.rs`, all 9 real hits, all behind `#[cfg(feature = "cranelift")]`
  (a narrower opt-in than the `aot` feature `test_aot.sh` builds — see
  verification note below): `cranelift_c_abi_wrapper_function` and
  `cranelift_standalone_main_wrapper` (5 `unwrap_call` total, repeated
  `wrapper.entry_block_mut().unwrap()` after `IrFunction::new`, which always
  creates its own entry block) → both functions now return
  `AotResult<ir::IrFunction>` and look up `entry_block_mut()` once via
  `.ok_or_else(|| AotError::InternalError(...))?`, with callers updated to
  propagate via `?`; `insert_builtin_cranelift_complex_layouts` (1
  `expect_call`, `cranelift_scalar_layout(&element_ty).expect(...)` for a
  hardcoded F64/F32 literal) → the general `Option`-returning helper is no
  longer called at all for this site — the known-scalar size/align is now a
  parameter, removing the panic path by construction; the Cranelift math
  builtin dispatch arm (1 `expect_call`, re-deriving a match guard's
  `Option` a second time) → `let Some(math_name) = ... else { return
  Err(AotError::InternalError(...)) }`; the Complex struct-field-offset
  lookup (2 `expect_call`, `layout.field("re"/"im").expect(...)`) →
  `.ok_or_else(|| AotError::InternalError(...))?`, matching the
  `ok_or_else` idiom already used two functions below
  (`lower_struct_field_assign`) in the same file.

Verification for the `#[cfg(feature = "cranelift")]` sites needed an extra
step beyond the standard gate: `test_aot.sh` builds `--features aot` only,
so it never compiles cranelift-gated code at all. Confirmed
`cargo check -p subset_julia_vm --features aot,cranelift` compiles clean
before and after, and that
`cargo clippy -p subset_julia_vm --features aot,cranelift --all-targets -- -D warnings`
reports the exact same 6 pre-existing, unrelated warnings both before and
after this change (`map_identity`, one `cast_sign_loss`, `single_element_loop`,
three `useless_conversion` — none touching the converted lines) — i.e. no
regression, though this feature combination is not itself a standing gate
and these 6 pre-existing warnings remain out of scope for this issue.

Deferred: 6 sites in `aot/codegen/aot_codegen/expressions.rs` are Rust
string *literals* — Julia-runtime codegen templates the AoT backend emits
into the **generated** Rust program (`dynamic_binop(...).unwrap()`,
`{}({}).unwrap()`, `dynamic_call(...).unwrap()`, `pop!`'s
`.expect("pop! from empty collection")`, `popfirst!`'s
`panic!("popfirst! from empty collection")`, `@elapsed`'s
`.expect("time went backwards")`), matching this document's
"Codegen string templates" caveat below verbatim. These are
**generated-code safety**, a different remediation kind from a live Rust
panic in `aot_codegen.rs` itself (the panic would happen inside the
*compiled Julia program*, at Julia run time, not inside the sjulia compiler
process) — per this issue's acceptance criterion, not converted here.
Follow-up: Issue #10955 (suggests routing each template through
`subset_julia_vm_runtime::error::aot_throw`, matching the dozen or so
sites in the same file that already do this for BoundsError/KeyError/
InexactError).

**Issue #10955 (DONE) — all 6 sites converted.** `dynamic_binop(...)`,
the multidispatch-dispatcher call, and `dynamic_call(...)` all return
`RuntimeResult<Value>` already, so their `.unwrap()` became
`.unwrap_or_else(|e| subset_julia_vm_runtime::error::aot_throw(e))` (`e:
RuntimeError` implements `Display`). `pop!`/`popfirst!` on an empty
collection now emit `aot_throw("ArgumentError: array must be non-empty")` —
matching both upstream Julia and the VM interpreter's own
`VmError::EmptyArrayPop` mapping (`vm/exec/error_handling.rs`) exactly.
`time_ns()`'s `SystemTime::now().duration_since(UNIX_EPOCH)` is the one
exception: a backwards-running host clock has no corresponding Julia
exception, so instead of inventing one, the template now mirrors the VM
interpreter's own `time_ns()` handler (`vm/builtins_io.rs::BuiltinId::TimeNs`,
which already used `.unwrap_or_default()`) rather than routing through
`aot_throw` — keeping AoT and interpreter behavior identical for this one
host-environment edge case. New e2e tests build and RUN the generated Rust
binary for the `pop!`/`popfirst!`-on-empty-array case and assert a non-zero
exit with `ArgumentError: array must be non-empty` on stderr (not the old
raw panic text) — `test_aot_pop_empty_collection_throws_julia_argument_error_10955`
/ `test_aot_popfirst_empty_collection_throws_julia_argument_error_10955` in
`subset_julia_vm/tests/aot_e2e_tests.rs`. The production-lane baseline's 3
`subset_julia_vm/src/aot` rows (Issue #10955) were removed from
`docs/vm/PANIC_FREE_PRODUCTION_BASELINE.tsv` (now 0 real hits, confirmed via
`panic_debt_classification.sh` — the module no longer appears in the
user-input-reachable top-modules list at all), and the corresponding
`PANIC_FREE_RATCHET_BASELINE.tsv` rows were tightened to their new exact
counts (`expect_call` 47→45, `panic_macro` 26→25, `unwrap_call` 638→633;
these three rows don't reach 0 because the ratchet also counts test-only
hits elsewhere in the same module path). Caveat found while authoring the
fix: a first draft of the in-code comment explaining the conversion
contained the literal substrings `.expect()`/`panic!()` in prose, which the
naive static-token classifier counted as new real hits in the very file
being fixed — reworded to avoid the tokens entirely (see this document's
own "Codegen string templates" caveat: the classifier has no comment/string
distinction beyond `mask_non_code()`'s string-literal masking).

**`aot_throw` (`subset_julia_vm_runtime/src/error.rs:148-150`) — evaluated,
not converted.** This crate is always built (`subset_julia_vm_runtime` is a
default-member, not feature-gated), so its one real `panic_macro` hit
(`pub fn aot_throw<T: Display>(e: T) -> ! { panic!("{}", e); }`) looked in
scope at first glance. Line-level classification found the other 2 raw
token matches this file's ratchet-baseline count (`panic_macro`=2,
`unwrap_call`=1 before this Issue's numbers) are doc-comment *mentions* of
`panic!`/`.unwrap()` in the function's own explanatory comment, not
additional real calls. The one real `panic!` is `aot_throw` itself — a
deliberate, issue-tracked (#5658, #7018) diverging function: it is the AoT
backend's sole mechanism for mapping an uncaught Julia `throw(e)` (and every
`BoundsError`/`KeyError`/`InexactError`/etc. template site above) to
"abort the compiled native binary", analogous to upstream Julia printing an
uncaught exception and exiting non-zero — except a compiled native binary's
closest equivalent to "exit with an error" *is* a controlled panic (kept in
this one runtime-crate function specifically so the AoT-generated code
itself never contains a raw `panic!`, per Issue #5658's design). Converting
it to e.g. `eprintln!` + `std::process::exit(1)` would be a *design* change
outside this debt-retirement issue's scope, not a mechanical panic-to-Result
conversion: it reverses a deliberate, already-reviewed decision, and breaks
the existing `#[should_panic(expected = "...")]` regression test
(`aot_throw_uses_display_text_issue_7018`) that pins this exact behavior.
Left unconverted; `subset_julia_vm_runtime/src/error.rs` was **not** added
to `docs/vm/PANIC_FREE_DENY_MODULES.tsv` (it legitimately still has a
`panic!`) and no `RULES_BY_FILE` classification-script rule was added for
it either — none of the four existing buckets (test-only /
build-time-invariant / cache-corruption-boundary / user-input-reachable)
honestly describes "intentional, already-reviewed accepted panic", so this
prose paragraph is its documented home instead of a script rule.

### Enforcement endpoint — Phase 3

**Status: DONE (Issue #10908).** The four front-door/runtime phases above
converted every real, reachable production `unwrap_call`/`expect_call`/
`panic_macro` site their scope named. Phase 3's job was to (a) inventory and
close the remaining production front-door gaps no earlier phase's scope
named, (b) expand `docs/vm/PANIC_FREE_DENY_MODULES.tsv` to the parent
`mod.rs` files Phase 1a/1b explicitly deferred (the child-cascade problem),
(c) turn the informal "production vs. test" distinction this document has
used throughout into a machine-checked gate, and (d) add the malformed-input/
corrupt-cache/repeated-eval process-survival tests this document's scope
disclaimer says are "the actual safety gate" — not the token counts.

**(a) New real production sites found and converted (4):**
- `subset_julia_vm/src/runtime_types.rs::build_reflection_inference_session`
  (`.expect("reflection inference factory must be installed before VM
  reflection")`) — proof-backed by `macro_runtime::install()`'s doc-commented
  guarantee ("every integration-crate composition root ... calls this before
  lowering"), same shape as the Phase 1c `repl/session.rs:550` precedent:
  converted to return `Option<Box<dyn ReflectionInferenceSession>>`; all 3
  production call sites (`vm/builtins_reflection/mod.rs`) already sat inside
  `Option`-returning functions and reuse their own "can't infer, fall back"
  `?` idiom — zero behavior change on any path that was already succeeding.
- `subset_julia_vm/src/bin/sjulia.rs`'s `--dump-ast --json` mode
  (`serde_json::to_string_pretty(&output).unwrap()`) — converted to the same
  `eprintln!` + `std::process::exit(1)` idiom the function's own
  `fs::read_to_string(...).unwrap_or_else(...)` two lines above already uses.
- `subset_julia_vm_runtime/src/convert.rs::to_char`
  (`s.chars().next().unwrap()` guarded by `s.len() == 1`) — proof-backed
  guarded unwrap, converted to `ok_or_else` per the "Option Unwrapping"
  approved pattern.
- `subset_julia_vm/src/expr_heads.rs::ExprHead::spec()` — evaluated and
  **not** converted to a `Result`-returning signature (would ripple through
  every one of its ~5 real call sites for a lookup that can only fail via an
  sjulia-compiler-author oversight, never user input): kept as a single
  documented `#[allow(clippy::expect_used)]`, the same shape as the
  pre-existing `vm/exec/mod.rs` SystemTime exception, and strengthened with a
  new exhaustive-match regression test (`expr_head_registry_covers_all_variants`)
  that fails to *compile* — not just to pass — if a future `ExprHead` variant
  is added without a registry entry.

**(b) Deny-module expansion (19 new rows, 125 total):** the concrete
front-door files no earlier phase named (`expr_heads.rs`, `runtime_types.rs`,
`api.rs`, `bin/sjulia.rs`, `subset_julia_vm_runtime/src/convert.rs`,
`compile/type_stability/mod.rs`, `vm/stack_ops.rs`) plus, now that Phase
1a/1b's remaining real sites are all converted, the parent `mod.rs`
cascades those phases explicitly deferred: `subset_julia_vm_parser/src/lib.rs`
(Phase 1a), and `subset_julia_vm_compile/src/compile/mod.rs` +
`subset_julia_vm_lowering/src/lowering/mod.rs` plus every intermediate subtree root
(`compile/expr/mod.rs`, `compile/expr/call/mod.rs`, `compile/expr/infer/mod.rs`,
`compile/abstract_interp/mod.rs`, `compile/ssa_ir/mod.rs`,
`compile/lattice/mod.rs`, `lowering/expr/mod.rs`, `lowering/expr/quote/mod.rs`,
`lowering/stmt/mod.rs`) (Phase 1b's deferral). A cascading `#![deny(...)]`
in a `mod.rs` reaches every descendant module transitively (confirmed
empirically, not just by rustc-reference reasoning) — since these subtrees'
real (non-test) content was already fully converted, the remaining cascade
risk was purely `#[cfg(test)]` blocks lacking a local
`#[allow(clippy::unwrap_used, clippy::expect_used)]`; each pragma addition
was verified by `cargo clippy -p subset_julia_vm[_parser] --all-targets -- -D
warnings` and any reported test-module violation got its local allow added
(roughly 35 files across both crates needed one). One schema-manifest file
(`compile/instr_wire_ids.rs`) needed its test module's allow, bumping
`CACHE_VERSION` 129 → 130 (no wire shape changed).

**(c) Production-lane gate:** `scripts/check_panic_free_production_baseline.sh`
+ `docs/vm/PANIC_FREE_PRODUCTION_BASELINE.tsv` — see `docs/vm/PANIC_FREE.md`'s
"Production-Lane Gate" section for the mechanism. Deliberately a *new*,
narrowly-scoped file/script pair reusing `panic_debt_classification.py`'s
bucketing by import, rather than restructuring the existing 82-row
`PANIC_FREE_RATCHET_BASELINE.tsv`'s schema — that file has three independent
consumers (`check_panic_free_ratchet.sh`, `panic_debt_classification.py`'s
own reconciliation, `check_audit_negative_selftest.sh`'s injection), and its
raw per-module counts remain a useful broad regression guard on their own
(any new panic source, in any bucket, still bumps some row). The 116
remaining production hits are all issue-linked in the new baseline file: 101
parser `self.expect(Token)` false positives (#10904), 6 AoT codegen-template
sites (#10955), 3 `aot_throw`/doc-mention sites in
`subset_julia_vm_runtime/src/error.rs` (#5658/#7018), the pre-existing
SystemTime exception, this phase's `expr_heads.rs` exception, and 3
doc-comment/rustdoc example false positives (`api.rs`, `vm/stack_ops.rs`,
`compile/type_stability/mod.rs`) — matching the "doc-comment examples"
caveat below.

**(d) Boundary process-survival corpora added** (no new
`subset_julia_vm/tests/*.rs` binary — every test landed in an existing
binary or the FFI crate's own `#[cfg(test)]` modules, per the
`TEST_BINARY_ALLOWLIST.tsv` policy):
- FFI (`subset_julia_vm_ffi/src/detailed.rs`):
  `test_compile_and_run_detailed_malformed_source_never_panics` sweeps 9
  malformed-source shapes (unterminated function/struct/for/if, unterminated
  matrix/call/let, embedded control characters, empty source) plus every
  prefix-truncation and single-character-deletion mutation through the real
  `compile_and_run_detailed` C ABI entry.
- FFI (`subset_julia_vm_ffi/src/bytecode.rs`):
  `test_run_vm_bytecode_detailed_bit_flipped_body_is_stale` XORs every byte
  past a generous header allowance of a real compiled `.sjvmbc` payload and
  asserts `run_vm_bytecode_detailed` still returns `CErrorKind::StaleBytecode`
  — closing the gap between the internal `vm_bytecode_file.rs` bit-flip test
  (Issue #10906) and the actual FFI C ABI entry.
- CLI (`subset_julia_vm/tests/sjulia_cli_tests.rs`,
  `sjulia_cli_malformed_source_survival_10908_tests` module): spawns the real
  `sjulia` binary against the same 9-shape malformed-source corpus (as a
  file argument, as `-e` argument, and every prefix truncation), plus an
  invalid-UTF-8 file and an empty file/argument — asserting the process
  exits non-zero (where a malformed shape is definitionally invalid) and
  stderr never contains `panicked at` (the shared substring of every Rust
  panic hook's default message).
- REPL repeated-eval survival
  (`subset_julia_vm/tests/regression_scope_session_tests.rs`'s
  `repl_session_malformed_redefinition_10906_tests` module):
  `repeated_malformed_eval_survives_across_many_iterations` cycles a single
  long-lived `REPLSession` through 7 malformed shapes 50 times (350 malformed
  evals total), interleaved with a valid eval after each round whose result
  must still be correct — proving a session that has already absorbed many
  malformed evals keeps behaving correctly indefinitely, not just that one
  malformed eval in isolation doesn't panic.

**Acceptance criteria evaluation:** see the PR body for #10908 (or the
epic #10869 thread) for the full per-criterion table; summary: deny-module
expansion done, production-baseline-zero done (modulo the issue-linked
allowlist), boundary corpora done, FFI/CLI/REPL process-survival tests done.
`indexing_slicing` (7,717 hits at the Issue #8705 baseline) remains
explicitly out of scope per this document's own scope disclaimer — Phase 3
did not attempt it, and no phase of this epic claimed to.

## Caveats found while classifying (documented so they are not re-discovered)

- **`self.expect(Token::X)` is not `Option::expect`**: see Parser section
  above — 101/102 of the parser's `expect_call` matches are a safe,
  already-checked helper method that merely shares the method name `expect`.
- **Codegen string templates**: `subset_julia_vm/src/aot/codegen/aot_codegen/expressions.rs`
  contains Rust string *literals* that are Julia-runtime source templates the
  AoT backend emits into the user's generated program (e.g. the `pop!`/`popfirst!`
  panic message text, `SystemTime::now().duration_since(...).expect(...)` for
  `@elapsed`). These count as `expect_call`/`panic_macro`/`unwrap_call` under
  the static regex (shared with `panic_free_inventory.py` and the ratchet —
  not specific to this script) but are not live Rust-level panics in
  `aot_codegen.rs` itself; a future, more precise inventory could carve this
  out as its own remediation kind ("emitted-code text", not "live call").
  **Update (Issue #10955, DONE):** all 6 of these template sites were
  converted (see the write-up above) — the same false-positive risk resurfaced
  during that work, but as an *in-code comment* containing the literal
  substrings `.expect()`/`panic!()` in prose (not a template string), which
  the classifier's line-level regex has no way to distinguish from a real
  call; reworded to avoid the tokens rather than teaching the classifier
  comment-awareness.
- **Doc-comment examples**: `core_ir_file.rs:34,37` and
  `compile/type_stability/mod.rs:34` match the static regex from inside a
  `//!`/`///` doc comment, not executable code — again a property of the
  underlying token scan shared by every script in this family, not introduced
  here.
- **`mask_non_code()` raw-string bug found and fixed during authoring**: the
  classification script's inline `#[cfg(test)]` brace-scope detector
  originally used an unanchored `r(#*)"..."` regex for raw strings, which
  misidentified an ordinary string ending in a word that ends in "r" (e.g.
  `.expect("parse/lower")` in `compile/preload_cache.rs:815`) as the *start*
  of a raw string, corrupting the brace count for everything up to the next
  unrelated `"` in the file (`compile/preload_cache.rs:816-834`, six
  `expect_call` sites wrongly bucketed as user-input-reachable instead of
  test-only). Fixed with a negative lookbehind requiring a non-word
  character before the `r`/`b` prefix; `scripts/panic_debt_classification.py`
  now also self-checks masked brace parity on every run and prints any
  residual imbalance as an informational diagnostic (5 files remain
  imbalanced by this measure at authoring time, all manually verified to
  still classify their real panic-token lines correctly — see
  `find_brace_imbalanced_files()`'s docstring).

## Phase sub-issues (epic tracking)

Filed as tracked sub-issues of #10869; each carries the `tech-debt` label,
references #10869, and is seeded with this document's per-module numbers and
acceptance criteria drawn from the epic body.

| Phase | Issue | Scope |
|---|---|---|
| 1a | #10904 (DONE) | Parser public entrypoints + CST accessors: zero-deny the parser crate's production modules. All 17 `subset_julia_vm_parser/src/parser/**` files converted (105 `unwrap_call` -> 0; the 101 `self.expect(Token::X)` checked-helper calls left as-is, confirmed not real debt); `lib.rs`/`cst.rs` opportunistically reached zero too and were added to the deny list (`subset_julia_vm_parser/src/lib.rs` itself was NOT deny-listed — its crate-root `#![deny(...)]` was found to cascade to every sibling module via `pub mod`, which is out of this phase's scope). Two genuinely-reachable panics found and fixed (`parse_identifier`/`parse_field_identifier` on truncated `struct`/`a.`); everything else was a proof-backed invariant, confirmed by an exhaustive truncation + character-substitution fuzz sweep before conversion. New corpus test: `subset_julia_vm_parser/tests/malformed_input_no_panic_tests.rs`. Found and filed 3 unrelated wrong-parse gaps in the same session (`::::` #10915, bare `->` #10917, bare `end` #10918) — not fixed here, out of scope for panic conversion. |
| 1b | #10905 (DONE) | Lowering/compile front door: convert to typed errors (`UnsupportedFeature`/`SpannedVmError`/internal invariant), add a malformed-input differential/fuzz test. All 49 real user-input-reachable sites converted (`lowering` 21, `compile` 19 real + 2 doc-comment regex false positives left as-is per the same reasoning as the parser's `self.expect(Token::X)` matches, `macro_runtime.rs` 9): new `internal_lowering_error`/`internal_compile_error` helpers (mirroring the parser crate's `internal_parser_error`) for guarded `.ok_or_else(...)?` conversions, plus several sites resolved with a pure type-level restructure needing no new error path at all (`compile/expr/infer/{julia_type,mod}.rs`'s `contains('{')` + re-`find` collapsed into one `if let Some(brace_idx) = ..`; `lowering/expr/collection.rs`'s `make_generator_clause_function` matches `vars`/the decoded binding directly instead of re-deriving a boolean; `compile/abstract_interp/worklist.rs::lower_block_to_cfg` returns `Option` so its 3 production callers reuse their own existing fallback idiom; `compile/abstract_interp/engine/mod.rs::record_backedge_module_call` binds `active_specialization` once via `let Some(..) = .. else { return }`, mirroring its own sibling function). The three flagged `lowering/mod.rs` `parsed_rust.as_ref().unwrap()` sites were verified provably safe (set unconditionally 1-2 lines above under an exclusive `&mut self` borrow) and converted to a guarded match with a zero-span internal error rather than left as bare unwraps. `compile/stmt.rs`'s six-times-repeated `self.loop_stack.pop().unwrap()` became one `pop_loop_frame()` helper method. 17 files reached zero and gained `#![deny(clippy::unwrap_used, clippy::expect_used)]` (registered in `docs/vm/PANIC_FREE_DENY_MODULES.tsv`); 8 parent `mod.rs` files whose deny would cascade into unconverted child modules were left out of scope, mirroring Phase 1a's decision to skip `subset_julia_vm_parser/src/lib.rs` for the same reason. New corpus test: `subset_julia_vm/tests/regression_misc_tests.rs`'s `lowering_compile_malformed_input_10905_tests` module (20 representative snippets, EVERY prefix-truncation and single-character-deletion mutation, run through the bare `Parser`+`Lowering`+production `host_support::compile_with_cache` pipeline inside `catch_unwind` — the Base-cache-seeded compile entrypoint the CLI/FFI/Web hosts use, which is what makes the exhaustive sweep affordable in-suite at 4-12s per test; the uncached `compile_core_program` harness paid a full Base re-inference per parse-able mutant, ~1.9s each / 10+ min per sweep, and its exhaustive truncation sweep was also run to completion during verification with zero panics). `compile/abstract_interp/engine/mod.rs` is a base-cache schema-manifest input, so `CACHE_VERSION` was bumped 128 → 129 (no serialized wire shape changed). |
| 1c | #10906 (DONE) | REPL/session + cache deserialize/load: zero-deny the malformed/redefinition and cache-load paths. |
| 2 | #10907 (DONE) | Runtime/optimization paths: `vm/specialize`, type ops, formatting, register VM, AoT — module-level zero-deny plus newtype/validated IR boundaries for `expect("compiler invariant")`-style sites. All 4 named VM modules (15 files) zero-denied (0 real hits, as Phase 0 predicted); `vm/executable.rs`'s 1 real stray hit converted; AoT: 16/22 real Rust-call sites converted (2 proof-backed invariants moved to a genuinely infallible API — `SpecializationQueue::ensure_enqueued` via `HashMap::entry()`, a merged `if let` binding in `StaticType::to_rust_type` — the rest to `AotResult`/`ok_or_else`); 6 AoT codegen-template string-literal sites (a different remediation kind — generated-code safety) deferred to Issue #10955; `subset_julia_vm_runtime`'s `aot_throw` panic evaluated and left as a documented, already-reviewed (#5658/#7018) design decision, not debt. |
| 3 | #10908 (DONE) | Enforcement endpoint: 4 new real production sites converted (`runtime_types.rs` reflection factory → `Option`, CLI `--dump-ast --json`, `subset_julia_vm_runtime/src/convert.rs::to_char`, `expr_heads.rs::spec()` documented exception); 19 new `PANIC_FREE_DENY_MODULES.tsv` rows (125 total), including the `subset_julia_vm_parser`/`compile`/`lowering` crate-subtree-root cascades Phase 1a/1b deferred; new production-lane gate (`check_panic_free_production_baseline.sh` + `PANIC_FREE_PRODUCTION_BASELINE.tsv`, registered in `source_only_audits.tsv`) isolates the `user-input-reachable` bucket from the broad ratchet and gates it at 0 modulo an issue-linked allowlist (116 rows: 101 parser false positives, 6 AoT codegen templates, 3 `aot_throw`/doc-mentions, 3 doc-comment examples, the SystemTime + `expr_heads.rs` exceptions); new FFI (`detailed.rs`/`bytecode.rs`), CLI (`sjulia_cli_tests.rs`), and REPL-repeated-eval (`regression_scope_session_tests.rs`) process-survival corpora, no new `tests/*.rs` binary. See "Enforcement endpoint — Phase 3" above for the full write-up. |
| 4 | #10979 (DONE) | vm/ top-level subtree deny-cascade: Phase 3's remaining size-driven deferral — `subset_julia_vm_vm/src/vm/mod.rs` itself — reached zero real hits (Phase 0's finding, re-confirmed) and gained `#![deny(clippy::unwrap_used, clippy::expect_used)]`, cascading to ~40 direct child files plus the 4 subdirectories (`builtins_reflection/`, `builtins_macro/`, `dynamic_ops/`, `matmul/`) Phase 2 had not individually denied; 17 `#[cfg(test)]` scopes (2 whole-file, 15 per-`mod`/`fn`) gained local `#[allow(...)]`; no production code changed. |
| 2 (AoT remainder) | #10955 (DONE) | The 6 AoT codegen-template sites Phase 2 (#10907) deferred as a distinct "generated-code safety" remediation kind: `dynamic_binop`/dispatcher-call/`dynamic_call`'s `.unwrap()` → `.unwrap_or_else(|e| aot_throw(e))` (all three already return `RuntimeResult<Value>`); `pop!`/`popfirst!` on an empty collection → `aot_throw("ArgumentError: array must be non-empty")`, matching both upstream Julia and the VM interpreter's `VmError::EmptyArrayPop`; `time_ns()`'s backwards-clock case → `.unwrap_or_default()` (mirrors the VM interpreter's own `time_ns()` handler; no corresponding Julia exception exists, so this one path intentionally does not use `aot_throw`). New e2e tests build+run the generated binary for `pop!`/`popfirst!` on an empty array and assert a non-zero exit with the Julia-shaped `ArgumentError` message on stderr, not the old raw panic text. Removed the 3 now-zero `subset_julia_vm/src/aot` rows from `PANIC_FREE_PRODUCTION_BASELINE.tsv` (116 → 110 justified hits) and tightened the matching `PANIC_FREE_RATCHET_BASELINE.tsv` rows to their new exact counts. See "Issue #10955 (DONE) — all 6 sites converted" above for the full write-up. |
