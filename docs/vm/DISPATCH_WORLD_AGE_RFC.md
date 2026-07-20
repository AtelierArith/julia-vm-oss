# RFC: A Unified World-Age-Aware Dispatch Resolver (Issue #10045)

**Status**: RFC — proposes a design direction and lists alternatives; not yet
implemented. Written as part of epic #10045's "no unified world-age-aware
dispatch resolver" structural-debt item. Nothing in this document ships code.

## Summary

sjulia currently has **three** partially-overlapping mechanisms that each
answer some version of "which method definitions are visible to this call,
right now": a genuine runtime world counter used by the value-based VM
dispatcher, a purely lexical (source-position) proxy used by the compiler
for top-level/script calls, and a separate world-range model used only for
inference-cache validity. Only the first is a faithful analogue of upstream
Julia's world-age model. The lexical proxy is the recurring source of the
nine incidents this document catalogs (#9400, #9650, #9742, #9979, #9990,
#9787, plus the three prevention issues #9998, #9992, #9987, whose guard
layer — checklists, an ordering audit, and boundary tests — landed in
PR #10086, 2026-07-10)
because source position is not world age — it is an approximation that
breaks whenever compilation order and lexical order diverge (REPL
accumulation, function-body calls to not-yet-lexically-later definitions,
callable-value dispatch). The proposed direction is to make the existing
runtime world counter the **only** visibility predicate, and to change the
compiler's lexical proxy from "decide visibility itself" to "detect when
static resolution isn't safe and defer to the (already-correct) runtime
predicate" — the same shape the function-body call path already uses.

## Current State: Three Mechanisms, One Concept

### Mechanism A — the real runtime world counter (VM dispatch)

`subset_julia_vm_vm/src/vm/state.rs` maintains a monotonic `current_world: u64`
counter (declared in `vm/mod.rs:1263`) that is bumped exactly once per
top-level method activation:

```rust
// subset_julia_vm_vm/src/vm/state.rs:2215-2220
pub(crate) fn activate_eval_function(&mut self, index: usize) {
    self.current_world = self.current_world.saturating_add(1);
    ...
    std::rc::Rc::make_mut(func).min_world = self.current_world;
    ...
}
```

Every VM call frame captures the counter's value **at the moment the frame
is pushed**: `frame.world_age = self.current_world` (`vm/mod.rs:1566`;
field declared `vm/frame.rs:101`, default `1` at `vm/frame.rs:127`). The
value-based runtime dispatcher consults exactly this captured value, never
the live counter:

```rust
// subset_julia_vm_vm/src/vm/state.rs:2202-2213
pub(crate) fn current_dispatch_world(&self) -> u64 {
    self.frames.last()
        .and_then(|frame| frame.func_index.map(|_| frame.world_age))
        .unwrap_or(self.current_world)
}

pub(crate) fn function_visible_in_world(&self, index: usize, world: u64) -> bool {
    self.functions.get(index).is_some_and(|func| func.min_world <= world)
}
```

`find_best_method_index_from_candidates` (`subset_julia_vm_vm/src/vm/dispatch.rs:733`)
filters every candidate through `function_visible_in_world(idx,
self.current_dispatch_world())` before scoring (`dispatch.rs:755-763`) — this
is a **correct, general** world-age filter: a method activated after the
current frame's captured world age is invisible to it, exactly matching
upstream's "current world age never exceeds the global counter, may run
arbitrarily behind it" rule (verified below).

### Mechanism B — the lexical source-position proxy (compile-time)

The compiler does **not** use Mechanism A for its own visibility decisions.
Instead, `subset_julia_vm_compile/src/compile/context.rs:41-49` defines a
source-position-keyed method row:

```rust
pub(crate) struct SourceOrderedMethodSig {
    pub(crate) sig: MethodSig,
    /// `None` = visible for the whole compilation unit (Base/cache/module
    /// rows). `Some(start)` = a top-level call only sees it at spans
    /// starting at or after that user method definition's *source offset*.
    pub(crate) visible_from_source_start: Option<usize>,
}
```

populated in `pipeline_ctx.rs:2445-2467` by comparing each function's
`func.span.start` against the current call site's span — a purely lexical
comparison, with no connection to `current_world`/`min_world`. Two
consumers branch on `strict_undefined_check`
(`compile/core_compiler.rs:590,661`; `true` inside function bodies, `false`
for top-level/module/script compilation):

- **`source_visible_method_table`** (`compile/expr/call/dispatch.rs:159-209`,
  used when `strict_undefined_check == false`, i.e. top-level/script/REPL
  statements): builds a **static, compile-time-fixed** method-table view by
  literally dropping every entry whose `visible_from_source_start` is later
  than the call site's own span start (`dispatch.rs:196-206`). This decision
  is baked into the compiled call — it never touches the VM's `min_world`
  at all.
- **`source_ordered_runtime_candidates`** (`dispatch.rs:87-157`, used when
  `strict_undefined_check == true`, i.e. inside function bodies): detects
  whether any two source-visible, same-arity, same-signature entries
  straddle the call site (`has_later_same_signature_redefinition`,
  `dispatch.rs:121-135`) and, only then, **defers to the runtime** by
  emitting the full candidate set (tracked + untracked methods,
  `dispatch.rs:140-156`) for `find_best_method_index_from_candidates` (i.e.
  Mechanism A) to filter by real `min_world` at call time.

So the two consumers do not even agree with each other: the top-level path
resolves visibility itself, lexically, at compile time; the function-body
path only decides **whether to bother the runtime**, and when it does, the
runtime's real world-age filter (Mechanism A) makes the actual call.

### Mechanism C — `WorldRange`, but only for the inference cache

`subset_julia_vm_compile/src/compile/abstract_interp/engine/world.rs` implements a
third, independent world model — `WorldRange { min_world, max_world }`,
explicitly modeled on upstream's `CodeInstance.min_world`/`max_world`
(`julia/Compiler/src/cicache.jl:24-26`, matching field-for-field). This is
used exclusively by the abstract-interpretation engine's inference-result
cache (`world.rs` module doc: *"provides the world-range half of that model
so cached inference results carry an explicit validity window"*) —
`grep -rl WorldRange subset_julia_vm/src` returns only
`abstract_interp/engine/{world,backedges,mod}.rs`. It never intersects with
Mechanism A or B; a method's *dispatch* visibility and a cached *inference
result's* validity window are tracked by two entirely separate counters.

## Incidents Caused By the Split

| Issue | What happened | Which mechanism was wrong/missing |
|---|---|---|
| #9400 | Top-level script: a call *before* a redefinition retroactively saw the *later* method (`julia`: world 2 then 101; sjulia: 101, 101) | Mechanism B (top-level) had no visibility filtering at all yet |
| #9650 | Script call through an earlier-defined function still saw a later callee redefinition retroactively | Same root cause as #9400, in a function-body call path — motivated adding Mechanism B's function-body half (`source_ordered_runtime_candidates`) |
| #9742 | An eval-redefined function kept executing its pre-redefinition body when called via a `Function` **value** (`map`/`broadcast`) | A third call shape (`CallFunctionVariable`) wasn't wired to either half of Mechanism B or to Mechanism A consistently |
| #9979 | After #9650's fix landed, `source_ordered_runtime_candidates`'s guard (`has_later_same_signature_redefinition`) fired for **ordinary multi-method overload sets**, not just genuine same-signature redefinitions, mis-dispatching `length(::FunctionValue)` and corpus/parity tests | Mechanism B's own guard was too coarse — "at least two source-visible same-arity entries" instead of "genuinely the same signature redefined" |
| #9990 | Same-arity overloads that were **all already visible before the caller** still forced Mechanism B's runtime-deferral path, widening `Base.infer_return_type` to `Any` even though nothing was actually ambiguous | Mechanism B deferred when static resolution was in fact safe |
| #9787 | REPL full-compile merges prior `eval`'d methods *after* the current input's functions; because those prior methods can reuse **source spans that overlap the current input's spans**, a fresh Persistent VM treated already-executed methods as not-yet-visible and failed runtime dispatch | Mechanism B is keyed on source **text position**, which is not stable across separate REPL compiles — the same span range means different things in different inputs. A real world-age counter (Mechanism A) has no such failure mode: `min_world` is a fact about *when a method was activated*, not where its source text happens to sit. |
| #9998 (closed by PR #10086) | Prevention issue naming exactly the #9990/#9787 pattern: *"source-order method activation must not hide already-executed methods"* | Names the structural fix this RFC proposes: stop keying visibility on source position |
| #9992 (closed by PR #10086) | Prevention issue: VM-side ad hoc runtime candidate sorting fixed the #9979 HOF symptom but perturbed package/metaprogramming call sites elsewhere, because the fix bypassed the shared resolver policy | Point fixes inside Mechanism A's scoring, instead of fixing the Mechanism B decision that produced a bad candidate set upstream of it |
| #9987 (closed by PR #10086) | `CallFunctionVariable`/`CallFunctionVariableWithKwargsSplat` execution (`vm/exec/call_function_variable.rs`) tried the legacy string-name scorer (`dispatch_function_variable`) as a *fallback after* Mechanism A's `find_best_method_index_from_candidates` returns `None` — correct in the code read at `call_function_variable.rs:1692-1710` for the plain `CallFunctionVariable` opcode, but #9987 documents call sites where this ordering wasn't yet consistently applied | A fourth, string-based scorer (pre-dating Mechanism A) still exists as a silent alternate path for some callable-value call shapes |

Six were closed by point fixes; the last three closed on 2026-07-10 when
their prevention deliverables (checklist entries, the
`check_call_function_variable_value_dispatch_order.sh` audit, and
compiler-boundary tests) landed in PR #10086. That guard layer pins
today's patched behavior — it is not the structural fix. The structural
gap is what this RFC addresses.

## Upstream Julia's World-Age Model

Verified against `julia/doc/src/manual/worldage.md` (Julia 1.12+; this repo's
upstream reference checkout):

- **One monotonic global counter**, incremented on every mutation to the
  global method table *or* the global binding table (method definition,
  type definition, `import`/`using`, constant/global declaration) —
  `worldage.md:20-22`. Retrieved via `Base.get_world_counter()`.
- **Each `Task` carries a local world age** ("current world age") that
  "will never exceed the global world age counter, but may run arbitrarily
  behind it" (`worldage.md:36-38`), retrieved via `Base.tls_world_age()`.
  Visibility of a method to a running call is decided by comparing the
  **executing task's** local world age against the method's valid range —
  not by anything captured on the function value itself.
- **The world age is constant across an entire ordinary function call
  chain.** Only specific syntactic events raise the *current task's* world
  age: the start of every top-level statement, the start of every REPL
  prompt, an explicit `Core.@latestworld`, and — but only *at top level* —
  type/struct/method/const/global/`using` definitions (`worldage.md:83-93`).
  Using any of these inside a non-top-level scope is a **syntax error**
  (`worldage.md:96-104`); this is what lets upstream state the invariant
  plainly: *"Julia may assume that the world age does not change within the
  execution of an ordinary function"* (`worldage.md:108-109`). A nested
  `@eval` still bumps the **global** counter (the `tryeval`/`newfun` example,
  `julia/doc/src/manual/methods.md:539-561`) but does **not** raise the
  calling task's local world age — so the newly `@eval`'d method stays
  invisible to the rest of that call chain, exactly what #9787's incident
  needed and Mechanism B could not express.
- **`invokelatest` is the one general, explicit escape hatch** — it
  temporarily raises the current task's world age for the duration of one
  call (`worldage.md:124-143`).
- **World age is captured at creation only for `Task`s and opaque
  closures**, not for ordinary function values in general
  (`worldage.md:263-287`, "World age capture"). This is a narrower claim
  than "every function value freezes its own world age" — an ordinary
  `Function` object dispatches using whatever the *calling* task's current
  world age is, not a value it captured when it was created or bound. This
  matters for the design below: sjulia's `frame.world_age` capture at VM
  call-frame push time is the correct analogue of *task*-scoped world age,
  not of "function values carry their own world age."
- **`CodeInstance` validity windows** (`min_world`/`max_world`,
  `julia/Compiler/src/cicache.jl:24-26`) are the *inference cache*'s use of
  the same counter — the direct upstream source for sjulia's Mechanism C —
  but method **dispatch** visibility in upstream is decided by the task's
  world age against the method table, not by `CodeInstance` validity; the
  two are related (an inference result is invalidated when the method table
  changes) but are not the same check.

The structural takeaway: upstream has **one** counter and **one** kind of
comparison ("is this task's current world age inside this method's/
binding's valid range?"), applied uniformly whether the call is at top
level, inside a function body, or through a function value. sjulia's
Mechanism A already reproduces this comparison faithfully at the VM level;
Mechanisms B and C are each solving a real problem (compile-time static
resolution, inference-cache validity) but neither should be a *second*
definition of "visible."

## Proposed Design: One Predicate, Reused Everywhere

1. **Formalize `function_visible_in_world` as the single visibility
   predicate.** It already exists and is already correct
   (`vm/state.rs:2209-2213`). No new counter is proposed — Mechanism A's
   `current_world`/`min_world`/`frame.world_age` triple is upstream-shaped
   and should not change.
2. **Replace Mechanism B's top-level path (`source_visible_method_table`)
   with the same shape Mechanism B's function-body path already uses.**
   Instead of building a compile-time-fixed, lexically-filtered method
   table (`dispatch.rs:159-209`), a top-level call site that cannot
   *statically* prove there is exactly one arity-matching candidate should
   emit the same deferred/dynamic candidate set
   `source_ordered_runtime_candidates` already builds
   (`dispatch.rs:140-156`), and let Mechanism A's real `min_world` filter
   decide — for both top-level and function-body call sites, through one
   code path. `visible_from_source_start` becomes a **compile-time
   heuristic for deciding when a fast static resolution is safe** (see Risk
   1 below), not a visibility source of truth in its own right.
3. **Retire the lexical span comparison as a source of correctness.** Once
   step 2 lands, whether two methods' source spans happen to overlap across
   separate REPL compiles (#9787/#9998's failure mode) stops mattering: a
   method's `min_world` is a fact recorded once, at the moment it was
   actually activated (`activate_eval_function`), independent of what
   source text a *later*, unrelated compile happens to reuse at the same
   offsets.
4. **Make Mechanism A's value-based scorer the only first-choice path for
   callable-value dispatch (#9987/#9992).** `call_function_variable.rs`'s
   plain `CallFunctionVariable` handler already tries
   `find_best_method_index_from_candidates` before falling back to the
   legacy `dispatch_function_variable` string scorer
   (`call_function_variable.rs:1692-1710`); the proposal is to make this
   ordering a structural invariant enforced at every `CallFunctionVariable*`
   opcode (kwargs-splat, splat) rather than a per-call-site pattern that a
   future opcode can silently omit — closing #9987's own proposed
   prevention item ("fails if any `CallFunctionVariable*` execution path
   selects methods with `dispatch_function_variable` before trying
   `find_best_method_index_from_candidates`") as a guarantee instead of an
   audit.
5. **Leave Mechanism C (inference-cache `WorldRange`) as-is.** It already
   matches upstream's `CodeInstance` model and answers a genuinely different
   question (cache validity, not dispatch visibility); this RFC does not
   propose merging it into the dispatch predicate, only naming the boundary
   clearly so a future change does not conflate the two the way B and A
   were conflated.

## Migration Steps

1. Add a shared "is this compile-time call site staticaly resolvable"
   predicate that both `source_visible_method_table` and
   `source_ordered_runtime_candidates` currently compute independently
   (`entries.iter().any(|entry| entry.visible_from_source_start.is_some())`
   plus the same-signature-straddle check) — deduplicate this logic first,
   without changing behavior, so step 2 has one call site to change.
2. Change the `strict_undefined_check == false` (top-level) branch to emit
   `source_ordered_runtime_candidates`'s dynamic candidate shape instead of
   `source_visible_method_table`'s filtered static table, gated behind a
   feature flag or `SJULIA_*` env var for a differential-comparison window
   (mirroring the existing `SJULIA_DISPATCH_COMPARE`/
   `SJULIA_BINARY_DISPATCH_COMPARE` compare-mode pattern already used for
   the typemap-filter migration, `dispatch_resolver.rs:114-163`).
3. Run the differential comparator across the full fixture suite and the
   dispatch parity corpus (`dispatch_parity_corpus_matches_upstream_julia_issue_8547`,
   named in #9979's failure list) to catch any top-level call site whose
   answer changes.
4. Flip the flag once the comparator is silent; delete
   `source_visible_method_table` and `visible_from_source_start`'s use as a
   filter predicate (keep the field only as the static-fast-path heuristic
   from step 1, if retained for performance — see Risk 1).
5. Enforce step 4's `CallFunctionVariable*` ordering with a structural test
   or compile-time assertion — PR #10086's
   `check_call_function_variable_value_dispatch_order.sh` audit already
   does this textually; a structural (compile-time) guarantee supersedes it.
6. Retire the now-redundant fixtures that specifically pinned Mechanism B's
   old lexical behavior *only if* their assertions still hold under the new
   path (`source_world_function_body_redefinition_9650.jl` must keep
   passing — it is a behavior fixture, not a mechanism fixture).

## Risks

- **Performance regression on the common case.** Today's static
  `source_visible_method_table` resolves the overwhelming majority of
  top-level calls (no redefinition in play) without touching the runtime
  candidate path at all. Routing every top-level call through the dynamic
  candidate shape unconditionally would add a runtime dispatch to call
  sites that provably need none. Migration step 1's "statically resolvable"
  predicate must stay a genuine fast path (single candidate, no
  same-signature straddle) — the proposal is to change what happens on the
  **multi-candidate/ambiguous** path, not to remove static resolution of
  the unambiguous path.
- **REPL delta/live-append interaction.** #9992 separately names a live
  delta-compile hazard (#9980: a rejected live append falls back to a fresh
  delta path that can "emit call sites without candidate payloads"). Any
  change to how top-level candidate sets are built must be exercised
  against the REPL differential suite specifically, not just the batch
  fixture suite.
- **Full-suite-only failure mode.** Every incident in the table above that
  involves dispatch caching or REPL state (#9979, #9787) was only visible
  in the **full** `cargo nextest run --release` run, not category runs —
  this is exactly the class of change `AGENTS.md`'s "Tests are wrapped and
  unfiltered" rule exists for; any implementation PR for this RFC must run
  the full suite, never a category slice, before merge.
- **Answer-preserving, not just crash-preserving.** The migration must
  produce the *same* dispatch decisions as today's patched behavior for
  every currently-passing fixture — the RFC changes which mechanism
  produces the answer, not the intended answer itself (e.g.
  `source_world_function_body_redefinition_9650.jl`'s expected output does
  not change).

## Alternatives Considered

- **A. Keep patching Mechanism B's guard conditions in place** (roughly
  what has happened through #9400→#9650→#9979→#9990→#9787). Rejected as the
  long-term direction: the three prevention issues (#9998, #9992, #9987,
  guard layer merged as PR #10086) show the pattern recurring after five
  point fixes;
  each new call shape (function values, kwargs-splat, REPL delta) needs its
  own patch under this approach.
- **B. Make the runtime resolver defensive enough to never trust the
  compiler's static decision** (i.e., always defer to Mechanism A, never
  attempt static resolution). This is Migration Step 2 pushed to 100%
  coverage with no fast path, and is rejected for the performance reason in
  Risk 1 — it trades a correctness bug for a universal dispatch-cache-miss
  tax on the common case.
- **C. Introduce a fourth, purely-compile-time world model that mirrors
  Mechanism A's counter exactly (a compile-time shadow counter).** Rejected
  as unnecessary complexity: the compiler does not need its own counter,
  because the question it actually needs to answer is narrower — "can I
  prove this call site has exactly one candidate regardless of world age?"
  — which the deduplicated static-fast-path predicate from Migration Step 1
  already answers without maintaining a second counter in sync with the
  first.

## Related Documentation

- `docs/vm/TYPE_VALUE.md` — sibling #10045 design document (unrelated
  subsystem: the `Type{T}` value-representation gap).
- `docs/vm/CACHE_ARCHITECTURE.md` — background on sjulia's caching layers;
  relevant contrast for keeping Mechanism C (inference-cache `WorldRange`)
  scoped separately from dispatch visibility.
