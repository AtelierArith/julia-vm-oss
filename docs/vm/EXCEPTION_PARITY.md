# Exception Taxonomy Parity (Issue #10813)

Issue #10813 is the owner epic for the claim that sjulia's exception
**types**, **raise layer** (catchable runtime throw vs. an uncatchable
parse/lowering/compile-time abort), and **catchability** diverge
systematically from upstream Julia, and that `Test.@test_throws` ignoring
its expected-type argument (Issue #10354) makes an entire class of
type-mismatch bugs invisible to the fixture suite. This document began as the
**Phase 0** evidence deliverable and now also records the completed Phase 3
prevention lane.

## Verdict summary

| Claim | Verdict | Evidence |
|---|---|---|
| 1. Exception TYPE diverges for the same construct | **Confirmed in Phase 0; paid down** | The four original type-only mismatches are fixed and retained as ratcheted sentinels. |
| 2. Raise LAYER diverges | **Confirmed in Phase 0; named evidence fixed** | #10406/#10511/#10593 now match; their MWEs remain regression sentinels. |
| 3. `@test_throws` ignored its expected type | **Fixed** | #10354 implements upstream `do_test_throws` forms; wrong-type expectations record Fail and exit non-zero. |

## Corpus probe (`scripts/exception_parity_probe.py` / `.sh`)

Runs a fixed corpus of error-triggering Julia
constructs under **both** upstream `julia` and `sjulia`, in `bare` and
`try/catch`-wrapped form, and records per-construct exception TYPE and
catchability parity to `docs/vm/EXCEPTION_PARITY_PROBE.tsv`. See the
script's module docstring for the full mechanism (sentinel-based wrapped
probe, why parse-error entries skip the wrapped run, timeout handling).

```bash
bash scripts/exception_parity_ratchet.sh --out docs/vm/EXCEPTION_PARITY_PROBE.tsv
```

### How to regenerate

Needs `julia` on `PATH`; the lane refreshes the REPL-feature release binary and
checks the issue-linked, two-sided allowlist in
`docs/vm/EXCEPTION_PARITY_ALLOWLIST.tsv`. New divergences fail, and fixed rows
fail as stale until removed. Interpreter startup failures, timeouts, signals,
non-zero sentinel exits, and missing sentinels fail independently of semantic
parity. `premerge_gate.sh --full-suite` runs this lane.

### Headline counts

| | Phase 0 (`c3d15ebf2`) | after Phase 2a (#11146) | Phase 3 (#11148) |
|---|---:|---:|---:|
| corpus cases | 40 | 40 | **46** |
| parse-time-only (catchability structurally n/a) | 2 | 2 | **2** |
| comparable cases | 38 | 38 | **44** |
| exact match (type + catchable) | 26 (68%) | 30 (79%) | **41 (93%)** |
| divergent | 12 (32%) | 8 (21%) | **3 (7%)** |
| — of which type-only mismatches | 4 | 0 | **0** |

**Every type-only mismatch in the corpus is now gone.** Phase 2a flipped
`convert_failure` and `method_error_noncallable`; PR #11163 (Issue #10354, merged
first) flipped `undef_var_call` and `memory_undef_ctor` — which is why Phase 2a
deliberately did NOT duplicate those two (see "Phase 2a outcome"). Phase 2b and
later bug fixes reduced the remaining rows to three issue-linked silent-error
gaps: #11559, #11794, and #11390.

### Phase 3 outcome (Issue #11148)

- The generic numeric-fallback sweep fixed `abs2` (#10602) and found the same
  dropped-`::Real` signature on `conj`/`isreal`/`flipsign` (#11522/#11525)
  and `real`/`signbit`/`abs` (#11797). All seven reject `String` at dispatch;
  the six sweep additions are permanent corpus sentinels and have
  numeric/Complex plus `applicable` control coverage in one fixture.
- The remaining static sweep candidates are tracked by #11799 rather than being
  silently routed around: five wrong-`TypeError`, ten uncatchable-internal-error,
  and one silent-`false` fallback.
- `parametric_ctor_nonconvertible` now puts its struct definition in probe setup
  and only its constructor call inside `try`. Wrapping the definition too had
  measured an unrelated top-level-definition restriction and falsely kept
  closed #10593 divergent.
- `exception_parity_ratchet.py` rejects new gaps, stale allowlist rows, changed
  divergence classes, invalid match tokens, corpus shrinkage, and same-count
  sentinel substitution. It also fails closed on either interpreter's
  infrastructure-health marker. Its unit matrix mutation-tests each boundary;
  `premerge_gate.sh --full-suite` owns the runtime wiring.
- The refreshed corpus has 44 comparable cases: 41 exact matches and three
  divergences, each owned by an open bug Issue. Fixture `@test_throws` checks
  exception types internally; this external lane independently compares the
  concrete type name and catchability produced by both runtimes.

### Phase 0 divergence breakdown (historical)

**Type-only mismatches (4)** — both sides catch it, wrong exception class:

| id | construct | julia | sjulia | note |
|---|---|---|---|---|
| `undef_var_call` | calling an undefined function name | `UndefVarError` | `ErrorException` | reading the same undefined name (not calling it) correctly raises `UndefVarError` on both sides — the divergence is specific to the *call* position. |
| `method_error_noncallable` | calling a non-callable value bound to a variable | `MethodError` | `TypeError` | same shape as the closed #10318/#10481 fixes (TypeError chosen as "nearest" error instead of MethodError); confirms the fix did not generalize into a shared funnel. |
| `memory_undef_ctor` | `Memory{Int64}(undef)` | `MethodError` | `UndefVarError` | Issue #10737 (open) — `undef` itself is not a defined global in sjulia. |
| `convert_failure` | `convert(Int, "a")` | `MethodError` | `TypeError` | same TypeError-vs-MethodError class Issue #10481 closed for `sqrt(::String)` — a second, independent instance survives on `convert`. |

**Silent / missing error (6)** — upstream raises, sjulia returns a value with no exception at all (the "silent wrong result" class Issue #10813 calls "more dangerous than a crash"):

| id | construct | julia | sjulia observed |
|---|---|---|---|
| `abs2_string_silent` | `abs2("a")` | `MethodError` | returns `"aa"` (Issue #10602, open) |
| `regex_match_oob` | `match(r"x", "abc", 10)` | errors | returns `nothing` (Issue #10736, open) |
| `regex_findnext_negative` | `findnext(r"\d", "abc", 0)` | `InexactError` | returns `nothing` (Issue #10736, open) |
| `domain_error_log_silent` | `log(-1.0)` | `DomainError` | returns `NaN` — no domain check at all (contrast with `sqrt(-1.0)`, which does check and matches upstream exactly) |
| `typed_local_reassign_no_enforcement` | `local z::Int = 1; z = "s"` | `MethodError` (via `convert`) | silently accepts, `z` becomes `"s"` — the declared local type is not enforced on reassignment |
| `undef_ref_error` | reading an unset `Vector{String}(undef, 1)` slot | `UndefRefError` | returns an already-populated value (not a genuine "undefined slot" read) with no error |

**Spurious error (1)** — sjulia raises where upstream succeeds:

| id | construct | julia | sjulia |
|---|---|---|---|
| `substitution_string_length` | `length(s"abc")` | succeeds | `MethodError` (Issue #10735, open) |

**True raise-layer divergence (1)** — sjulia never reaches the `catch` block at all (the program aborts before running, per Issue #10813 claim 2):

| id | construct | julia | sjulia |
|---|---|---|---|
| `parametric_ctor_nonconvertible` | `struct B{T}; x::T; end; B{Float64}("abc")`, evaluated **inside** `try/catch` | `MethodError` (catchable) | compile-time abort; the `try` block's own body never executes (Issue #10593, open) |

### Regression sentinels (closed issues, re-verified with their original MWEs)

Three of the epic's own Evidence-table issues are closed. Re-running their
**exact** original MWEs through the corpus (not a paraphrase) confirms all
three now match upstream exactly — these corpus entries are kept
permanently as regression sentinels so a future change that reopens one of
them shows up as a new divergent row:

- `immutable_field_assign` (#10511) — static immutable-field assignment now raises a catchable `ErrorException` on both sides.
- `getfield_module_bogus` (#10318) — `getfield(Base, :bogus)` now raises `UndefVarError` on both sides.
- `sqrt_string` (#10481) — `sqrt("a")` now raises `MethodError` on both sides.
- `map_dispatch_failure` (#10406) — the exact original MWE, `map(sqrt, ["a", "b"])` inside `try/catch`, now raises a catchable `MethodError` on both sides (the original bug report showed an uncatchable top-level `Runtime error` that skipped the `catch` block entirely).

This is direct evidence for the "confirmed, but narrowing" verdict on claim
2: the raise-layer problem was real and is being paid down issue by issue;
it is not evidence that it is unowned or growing.

## `@test_throws` type-check impact

### Root cause (claim 3)

`subset_julia_vm/src/julia/stdlib/Test/src/Test.jl`'s `@test_throws` macro:

```julia
macro test_throws(T, ex)
    quote
        local recorded = 1
        try
            $(esc(ex))
            _test_record!(false, "expression throws expected exception")
            recorded = 1
        catch e
            _test_record!(true, "expression throws expected exception")   # <- T never checked
            recorded = 0
        end
        _test_result(recorded)
    end
end
```

The `catch` branch records Pass unconditionally — `T` is bound but never
read. Already tracked as Issue #10354 (open, `bug`), which independently
diagnosed the same root cause and additionally found the `Memory{T}(undef)`
divergence (`memory_undef_ctor` above) it was masking. This document adopts
#10354 as this epic's Phase 1a and adds the fixture-fallout measurement
below.

### Fixture usage

`@test_throws` appears in **161 fixture files / 630 total call sites**
under `subset_julia_vm/tests/fixtures/`, spread across 44 category
directories (`types` 15, `generator` 11, `array` 10, `dispatch` 9, `macros`
8, ... down to 1 in each of ~20 smaller categories). Every one of these call
sites currently records Pass regardless of the thrown type.

### Measurement method

A throwaway patch (**not shipped in this PR** — applied, measured, and
reverted; `git diff` on `Test.jl` is empty in this PR) added the upstream
`isa`-check:

```julia
catch e
    local __ok = e isa $(esc(T))
    _test_record!(__ok, "expression throws expected exception")
    recorded = __ok ? 0 : 1
end
```

Both a baseline (unpatched) and a patched `sjulia` (`--profile dev-fast`,
built in an isolated `CARGO_TARGET_DIR` so neither build disturbed the
shared worktree target other concurrent agents use) were run over all 161
fixture files, comparing process exit code.

### Result

**8 of 161 files newly fail** (exit 0 → exit non-zero) under the patched
binary; **21 individual `@test_throws` assertions** flip from Pass to Fail
across those 8 files.

One of those 8 files, `regex/regex_recursion_reject_10181.jl` (8 of the 21
assertions), is a **measurement artifact of the throwaway patch, not a real
divergence**: it uses upstream `Test`'s second `@test_throws` form —
`@test_throws "message substring" expr` (checks the exception's message
contains a string, not `isa T`) — which the naive `e isa $(esc(T))` patch
does not handle (`e isa "recursion"` is not a valid `isa` call). A real fix
for #10354 must implement **both** the `Type` and the `String`/message-
substring forms of upstream `do_test_throws`, or this file's outcome is
undefined rather than correctly Pass. This is itself useful Phase 0 evidence
handed to Phase 1a below.

Excluding that file, **7 files / 13 individual assertions** are the real
fallout. Every one was individually attributed by extracting the exact
failing construct (with its full preceding file context — struct/function
definitions, `const` bindings — preserved) and running it standalone under
both `julia` and the baseline `sjulia`, comparing upstream's actual thrown
type against the fixture's declared expected type:

| Fixture | Assertions | Declared expected | Upstream actual | sjulia actual | Verdict |
|---|---:|---|---|---|---|
| `array/ncat_double_semicolon_line_wrap_10519.jl` | 1 | `ArgumentError` | `ArgumentError` | `ErrorException` | sjulia bug |
| `dispatch/subtype_isa_arity_5493.jl` | 4 | `ArgumentError` (all 4) | `ArgumentError` (all 4) | `TypeError` (all 4) | sjulia bug |
| `generator/generator_trait_matrix_9566.jl` | 3 | `ArgumentError` (all 3, the "flatten/length" cases) | `ArgumentError` (all 3) | `MethodError` (all 3) | sjulia bug |
| `memory/memory_single_arg_methoderror_10324.jl` | 1 | `MethodError` | `MethodError` | `UndefVarError` | sjulia bug (= Issue #10737) |
| `memory/test_memory_primitive_boundary.jl` | 1 | `ArgumentError` | `ArgumentError` | `TypeError` | sjulia bug |
| `modules/module_selective_using_globals_7955.jl` | 1 | `UndefVarError` | `UndefVarError` | `ErrorException` | sjulia bug (= `undef_var_call` above) |
| `types/signature_forward_reference_11025.jl` | 2 | `UndefVarError` (both) | `UndefVarError` (both) | **`String`** (both) | sjulia bug (see below) |

**13/13 = 100% genuine sjulia bugs, 0% fixture over-specification.** This is
the strongest single piece of evidence for claim 3: every one of these
assertions was silently passing while asserting something false about
sjulia's actual behavior, and would have failed loudly (matching upstream)
had #10354 been fixed when these fixtures were written.

The `signature_forward_reference_11025.jl` case is worth calling out on its
own: `typeof(e)` for the caught value is literally `String`, not any
exception type at all. sjulia's `eval`-time method-signature elaboration
(`eval(:(f(x::NotYetDefined) = 1))`) surfaces this failure as a **raw thrown
string**, not a typed exception object — a third, distinct defect
(untyped/stringly-typed error propagation) layered under the type-taxonomy
problem, found only because this Phase 0 measurement forced the exception
value to be inspected at all.

## Priority ranking

Ranked by user impact, following the epic's own framing (catchability
breaks > wrong type > message text):

| Rank | Class | Why this rank | Phase |
|---|---|---|---|
| 1 | `@test_throws` blind spot | The amplifier: while unfixed, every other row in this ranking stays invisible to the fixture suite by construction. 13 confirmed real bugs were hiding behind it in this repo's own tests alone. | 1a (adopts #10354) |
| 2 | Silent / missing error (6 corpus instances + `typed_local_reassign_no_enforcement`) | Worse than a crash per the epic's own framing — the program keeps running on a value it should never have produced. No `try/catch` placement saves you; there is nothing to catch. | 3 |
| 3 | Exception TYPE mismatch (4 corpus instances + 12 of the 13 fixture-fallout instances) | Catchable on both sides, so `try/catch`-based code doesn't crash — but type-`isa`-dispatching `catch` blocks (the upstream-idiomatic pattern) silently take the wrong branch. Recurs across independently-fixed sites (#10481 fixed `sqrt`, not `convert`), confirming Issue #10813's "no funnel" diagnosis. | 2a |
| 4 | Raise-layer divergence (1 open corpus instance, #10593) | Already the minority case (2 of 3 named instances closed); still worth a dedicated sweep since "uncatchable at all" is the most severe individual outcome when it happens. | 2b |
| 5 | Enforcement | Without a ratchet, rows 1-4 recur; this is process, not product, but is what keeps the count from drifting back up. | 3 |

## Phase sub-issues (epic tracking)

Filed as native GitHub sub-issues of #10813, each carrying the `tech-debt`
label and referencing "Part of #10813":

| Phase | Issue | Scope |
|---|---|---|
| 1a | #10354 (adopted; pre-existing, now a linked sub-issue of #10813) | Fix `@test_throws` to check `isa T` **and** the message-substring `String`/`Regex` form (the `regex_recursion_reject_10181.jl` artifact above shows both are needed); then fix (or file individually, if any turn out non-trivial) the 7 fixtures / 13 assertions this document's measurement attributed as genuine sjulia bugs. |
| 2a | #11146 — **DONE**, see "Phase 2a outcome" below | Exception-type taxonomy funnel: one `VmError` → upstream-exception-type table/constructor funnel; audit script forcing new error construction through it (Issue #10813's own P1 proposal). Seeded with this document's 4 type-only corpus instances + the fixture-fallout table's TypeError/ErrorException/MethodError/UndefVarError instances. |
| 2b | #11147 | Raise-layer parity sweep: catalogue remaining compile/lowering-time aborts upstream treats as catchable runtime errors, starting from #10593 (open); use `immutable_field_assign`/`getfield_module_bogus`/`map_dispatch_failure` as regression sentinels the sweep must not reopen. |
| 3 | #11148 | (a) Base generic-fallback signature sweep for the `abs2`/#10602 "untyped fallback silently succeeds" shape; (b) wire `scripts/exception_parity_probe.sh` into a ratchet (no new divergent row without an issue-linked allowlist entry, mirroring `docs/vm/PANIC_FREE_RATCHET_BASELINE.tsv`'s shape) and extend `scripts/fixture_julia_parity.sh` to compare exception **type** names, not just pass/fail counts, for `@test_throws`-bearing fixtures (Issue #10813's own P0 proposal bullet). |

All four are linked as native GitHub sub-issues of #10813
(`gh issue view 10813 --json subIssuesSummary`).

**Sequencing note for whoever picks up 1a**: landing the `isa T` check alone
red-lines the 7 fixtures / 13 assertions this document measured (their
declared expected type is correct; sjulia's current thrown type is not) —
the harness fix and the type fixes are not independent. Phase 1a must either
pull in the corresponding Phase 2a type corrections for those 7 fixtures, or
explicitly triage/skip them in the same PR that lands the `isa` check, or
the suite goes red on merge.


## Phase 2a outcome (Issue #11146, done)

### Scope judgment (stated with the evidence that drove it)

The issue seeded Phase 2a with "the 4 type-only corpus instances". Two of those
four — `undef_var_call` and `memory_undef_ctor` — were ALREADY fixed on the
in-flight PR #11163 (Issue #10354), which had not yet merged when this work
started. Their fixes there add a new `Instr` variant (`ThrowUndefVarError`) and a
`CACHE_VERSION` bump; re-implementing them independently would have produced
wire-ID and cache-version conflicts for no behavioural gain. They were therefore
NOT duplicated. Phase 2a fixed the other two (`convert_failure`,
`method_error_noncallable`) plus the funnel that makes the whole class
structurally impossible. Merge #11163 first; the probe's type-mismatch count then
goes to 0.

### The funnel

`VmError::exception_class()` (`subset_julia_vm_bytecode/src/error.rs`) is now the
ONE mapping from an internal `VmError` variant to its upstream Julia exception
class — a compile-time-exhaustive match with no catch-all arm. Before it, three
independent places decided what a raised error "is":

1. the raise site (variant + free-form message — which could NAME a different
   class than the variant it raised);
2. `vm_error_to_exception_value`, which hard-coded a Julia struct-name literal
   per arm;
3. `is_catchable_vm_error`, a hand-synced variant list whose doc comment asked
   the reader to "keep this list byte-for-byte in sync" — a convention.

(2) and (3) are now *derived* from the funnel: the exception object's struct name
comes from `ExceptionClass::julia_name()` and catchability is
`julia_name().is_some()`. So a raise site no longer picks a class at all — it
picks a variant, and the class follows; and adding a `VmError` variant does not
compile until its class is declared.

**What is structural vs. enforced.** The exception OBJECT's class is now a
compile-time guarantee. The message is a free-form `String`, which no compiler
can constrain — so the #10354 shape (`VmError::TypeError` whose text opens
`"ArgumentError: "`) is prevented by `scripts/check_exception_taxonomy_funnel.sh`
instead, which is the "equivalent enforcement" the phase called for, not a
compile-time guarantee. Stated plainly so the guarantee is not oversold.

### Fixed here

| Construct | was | now (= upstream) |
|---|---|---|
| `convert(Int, "a")` and every other `convert_to_*` fallback, all widths | `TypeError` | `MethodError` |
| `z = 5; z(1)` (4 call paths + function composition) | `TypeError` | `MethodError` |
| wrong-arity `<:` / `isa` | `TypeError` w/ "ArgumentError: " text | `ArgumentError` |
| negative `Memory` size | `TypeError` w/ "ArgumentError: " text | `ArgumentError` |
| `Memory` OOB get/set (7 sites) | `TypeError` wrapping a real `IndexOutOfBounds` | `BoundsError` |
| mid-character byte index (2 sites) | `TypeError` w/ "StringIndexError: " text | `StringIndexError` |
| invalid `Enum` value | `TypeError` w/ "ArgumentError: " text | `ArgumentError` |
| string slicing / tuple index misuse (6 sites) | `TypeError` w/ "ArgumentError:"/"MethodError:" text | `ArgumentError` / `MethodError` |
| LinearAlgebra shape checks (4 Rust + 29 Julia sites) | `ErrorException` w/ "DimensionMismatch: " text | `DimensionMismatch` |
| `Meta.parse` failure | `TypeError` w/ "ParseError: " text | `ParseError` |
| 35 pure-Julia `error("ArgumentError: ...")` sites | `ErrorException` | `ArgumentError` |

The Julia-layer conversions leave the RENDERED text byte-identical
(`ErrorException`'s `showerror` prints the bare message, so the `"<Class>: "`
prefix was already doing the class's job in text only) — only `typeof(e)` changes,
which is the entire point.

### The 2 bugs handed over from Phase 1a

`types/signature_forward_reference_11025.jl`'s two assertions were not merely
catching the wrong TYPE — they were catching something that was not an exception
at all (`typeof(caught) == String`). Root cause: `eval`'s runtime
method-definition path never implemented typed parameters, raised
`VmError::NotImplemented`, and — per Issue #8664's mapping — that variant had no
Julia exception object.

Both are FIXED, on the upstream-shaped path rather than by remapping
`NotImplemented`: upstream evaluates signature annotations EAGERLY when the
definition executes, so an unbound name raises `UndefVarError` at the definition.
The compiled path already mirrored this (`Instr::LoadAny` probes, #10396/#11025);
the runtime-`eval` path skipped it. It now probes the same names (every parameter
annotation and every `where` bound, minus the binders), and both fixture
assertions were STRENGTHENED from the vacuous `@test_throws` to a checked
`try`/`catch` + `isa UndefVarError`.

Independently (and because the funnel's invariant is "catchable ⇔ has an
exception object"), `NotImplemented` was reclassified from "no exception object"
to `ErrorException`: it is *user-reachable* (any construct sjulia has not
implemented), so binding a raw `String` in a user's `catch` was never defensible.
#8664's rationale ("no upstream equivalent") justified the class choice, not the
absence of one. A typed-parameter `eval` definition whose annotation RESOLVES
still raises (defining it as `::Any` would silently mis-dispatch — the
silent-wrong-result class this epic calls more dangerous than a crash); that
residual feature gap is tracked separately.

### Enforcement

`scripts/check_exception_taxonomy_funnel.sh` (registered in
`scripts/source_only_audits.tsv` with `premerge_default=true`, four negative
self-tests in `check_audit_negative_selftest.sh`):

- **R1** no `VmError::<Variant>(...)` may carry a message opening with a Julia
  exception class name that contradicts the variant's class. *(This rule found
  two `StringIndexError` sites a manual scan had missed.)*
- **R2** the catch-time exception builder may not hard-code a struct-name literal.
- **R3** the funnel's match may not gain a `_ =>` catch-all, and
  `is_catchable_vm_error` must delegate to it.
- **R4** the pure-Julia layer may not raise a class by naming it in an
  `error("<Class>: ...")` message. The 58 sites that still need constructor
  ARGUMENTS (`BoundsError(a, i)` wants the container and index; 54 BoundsError, 2
  DomainError, 2 InexactError) are ratcheted against
  `docs/vm/EXCEPTION_TAXONOMY_JULIA_BASELINE.tsv` — the count can only shrink —
  and retired in a tracked follow-up.

The audit PARSES the funnel to build its variant→class table, so it cannot
disagree with the code it guards, and it fails loudly rather than vacuously if
its targets move (the #9129 F2 failure mode).

## Caveats

- The corpus (46 hand-written cases) is illustrative, not exhaustive. It
  deliberately targets the exception classes Issue #10813 names (undefined
  var, MethodError, BoundsError, DivideError, InexactError, ArgumentError,
  TypeError, KeyError, DomainError, StackOverflow, parse errors, type-assert
  failures, kwarg errors) plus the concrete MWEs from its Evidence table.
  Phase 2a/2b's sweeps will surface instances this corpus does not cover.
- `method_error_noncallable`'s first corpus draft used the literal `(5)(1)`,
  which is **not** a non-callable-value call in Julia — a bare numeric
  literal directly before `(` is multiplication (`2(3) == 6`), and both
  interpreters agree on that. Fixed to bind the value to a variable first
  (`z = 5; z(1)`) to force a genuine non-callable dispatch. Left in this
  document as a caveat since it is exactly the kind of corpus-authoring trap
  future extenders of this script should watch for.
- "Parse-time-only" entries (`parse_error_dangling_op`,
  `parse_error_unmatched_paren`) do not run the wrapped try/catch probe at
  all: malformed syntax prevents the whole file from parsing regardless of
  whether the malformed construct is textually inside a `try` block, so
  catchability is `n/a` by construction, not measured.
