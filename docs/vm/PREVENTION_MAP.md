# Prevention Map: Parser/Lowering/Syntax root-cause verification (Issue #10983)

**Purpose**: this doc is the durable record for the adversarial verification
pass over Issue #10983 ("Prevention: Parser/Lowering/Syntax milestone bugs
share five architectural root causes"). #10983 cites
`memory/project/parser-lowering-syntax-root-causes-2026-07.md` as its
supporting investigation; that file was never committed (`git log --all`
finds no trace of it). This doc replaces that dangling pointer with a
checked-in, evidence-backed verdict per claimed root cause, so the umbrella
issue's narrative doesn't rest on an unreachable artifact.

Scope note: #10983 spans the parser, the type system, both macro-expansion
engines, and lowering proper — broader than `LOWERING.md`'s remit, and
`LOWERING.md` is mid-edit under PR #10981 (the #10627 macro-engine
convergence, at the lead's merge gate) — so this cross-cutting note lives on
its own rather than as a `LOWERING.md` section.

## Method

For each of #10983's five clusters (A–E): read the cited bug Issues, read
the actual fix commits/PRs where one exists, grep the implicated source, and
where a repro was given, re-run it live against both upstream `julia`
1.12.6 and `target/dev-fast/sjulia` at the verification HEAD
(`a891079ef044cbe4bc5fc841e23d6a2269ef066c`, 2026-07-14; `target/release/sjulia`
was not present in this worktree, so `dev-fast` was used — noted per
cluster below). Verdict axis (does the claimed mechanism explain the cited
bugs) is kept separate from the ownership axis (is there already a tracked
issue/epic driving the fix).

## Verdict table

| Cluster | Verdict | Evidence | Owner |
|---|---|---|---|
| **A. Scope/context flat maps** | CONFIRMED — real, but ownership is split | See "Cluster A" below | Macro hygiene → **#10925**. `LambdaContext` routing → **#10936** / **#10965**. `CoreCompiler` locals scope stack → **#10984 (fixed for `for`/`foreach`/`ForEachTuple`/single-var and tuple-destructuring comprehension, fixes #10903; multi-generator comprehension #11000 and exception-unwind #11001 residuals decomposed out)** |
| **B. Static/dynamic macro path desync** | CONFIRMED — already substantially owned; #10983 doesn't cross-link the owning epic | See "Cluster B" below | **#10627** (PR #10981, Pass-1 convergence, at the lead's gate) + **#10916** / **#10980** (Pass-2 gaps) |
| **C. Token syntactic-operator role** | CONFIRMED — live MWE reproduced | See "Cluster C" below | **#10940** (pre-existing, filed same day, already correctly cross-linked by #10983) |
| **D. Declaration-statement normalization** | CONFIRMED — 2 concrete instances already fixed same day | See "Cluster D" below | **#10951** (pre-existing, filed same day, already correctly cross-linked by #10983) |
| **E. Where-binder shadowing in Type annotations** | CONFIRMED, including the specific mechanism #10983 names | See "Cluster E" below | **#10436** (pre-existing epic, filed 2 days earlier, design flaws #2/#4 already describe this pattern); #10983 doesn't cross-link it |

### Timing context

All five clusters' "owning" issues were filed **before** #10983 itself:

| Issue | Created (UTC) |
|---|---|
| #10436 | 2026-07-11T01:57:58Z |
| #10627 | 2026-07-11T16:31:02Z |
| #10916 | 2026-07-13T13:28:15Z |
| #10925 | 2026-07-13T14:10:41Z |
| #10936 | 2026-07-13T15:12:53Z |
| #10940 | 2026-07-13T15:22:26Z |
| #10942 | 2026-07-13T15:24:04Z |
| #10951 | 2026-07-13T16:43:57Z |
| #10965 | 2026-07-13T18:49:44Z |
| #10980 | 2026-07-13T21:19:01Z |
| **#10983** | **2026-07-13T22:04:32Z** |
| #10984 (new, this PR) | 2026-07-14 |

#10983 is a same-day retrospective synthesis filed *after* most of the
individual architecture issues it describes. That is legitimate and
valuable as a narrative/cross-reference document — but it means the
"Prevention / fix plan" section of #10983 largely restates work already
tracked elsewhere, and the decomposition task is mostly "link correctly,"
not "file new issues."

## Cluster A — Scope/context flat maps

Three sub-claims, verified independently:

1. **Macro-hygiene rename is a flat whole-tree substitution.** Confirmed by
   #10925's own investigation (`macro_runtime.rs::rename_quote_local_symbols`,
   `lowering/expr/quote/hygiene.rs::HygieneContext::resolve`) — filed as its
   own tracked tech-debt issue before #10983. No new issue needed.
2. **`LambdaContext` is a grab-bag of unrelated concerns, switched on/off by
   a syntactic predicate (`contains_macro_call`).** Confirmed by reading
   `subset_julia_vm_lowering/src/lowering/mod.rs:468-525`: `LambdaContext` holds
   `lifted_functions`, `usings`, `macros`, `compile_time_functions/structs/
   abstract_types/primitive_types`, `macro_expanded_structs/macros`,
   `module_macro_hygiene`, `macro_hygiene_stack`, `current_file`,
   `active_type_params`, `active_value_params`, `current_module_stack`, and
   `prefer_nested_lambdas` — one struct owning macro state, lifted-lambda
   collection, hygiene, where-binder/value-param scoping, and a behavioral
   mode flag. Already tracked by **#10936** (routing exhaustiveness) and
   **#10965** (capability separation from closure-lowering mode), both
   filed before #10983 with near-identical root-cause language. **Caveat**:
   `active_type_params`/`active_value_params` are already
   `RefCell<Vec<HashSet<String>>>` — i.e. already scope-stack-shaped for
   where-binders/value-params specifically. The "no scope frames" framing
   is accurate for macro hygiene and `CoreCompiler.locals` (below), not
   universally true of everything `LambdaContext` touches.
3. **`CoreCompiler` has a flat, function-wide `locals: HashMap<String,
   ValueType>` with no lexical scope stack.** Confirmed by reading
   `subset_julia_vm_compile/src/compile/core_compiler.rs:46-98` — `locals`,
   `initialized_locals`, `julia_type_locals`, and
   `known_any_rank_array_locals` are all flat, name-keyed, function-wide.
   `local_scope_depth: usize` (`core_compiler.rs:141`) looked like a
   candidate scope mechanism but is a **counter**, not a frame stack — all
   6 call sites in `compile/stmt.rs` (lines 1933, 1945, 2157, 2923, 2998,
   3069-3072, 3589) use it only to decide whether strict-undefined-check
   semantics apply, not to isolate per-block bindings. Live MWE
   reproduction (`target/dev-fast/sjulia` @ a891079ef):

   ```julia
   function f()
       i = "outer"
       for i in 1:3
       end
       return i
   end
   println(f())
   ```

   | Runtime | Output |
   |---|---|
   | `julia --startup-file=no` (1.12.6) | `outer` |
   | `target/dev-fast/sjulia` | `4` |

   This was Issue **#10903**, tracked only as a bug symptom until **#10984**
   closed the architecture gap analogous to #10925/#10936/#10965's sibling
   coverage of the other two sub-causes, cross-linking two same-pattern-
   different-code-path bugs that corroborate this is a repo-wide shape (not
   a #10903-only defect), each separately owned in its own milestone and
   NOT fixed by #10984: **#10369** (dynamic-macro-path `catch`-variable
   leak, milestone "REPL and Module Evaluation State") and **#10523** (AoT
   sibling-`for`-loop slot unification, milestone "AoT Backend Expansion").

   **#10984 resolution** (2026-07-14, `Fixes #10903`): rather than the full
   nested-scope-stack rewrite #10984's issue body originally proposed —
   which would touch all ~30 name-keyed `CoreCompiler` fields and every
   `store_local`/`load_local` call site, the same explosion risk that has
   kept #10925/#10936/#10965 open — the landed fix generalizes the
   codebase's own PRE-EXISTING per-name shadow/restore idiom
   (`Expr::LetBlock` handling in `compile/expr/mod.rs`, which already fixed
   the analogous #1361/#9313/#7570 let-scope bugs) into two reusable
   methods, `CoreCompiler::shadow_local_enter`/`shadow_local_exit`
   (`compile/core_compiler.rs`): on a collision with a live outer local,
   snapshot its runtime value + compile-time type state to a fresh temp
   slot, then restore both at the construct's single normal/`break`-exit
   convergence point (a no-op when there is no collision). Wired into
   `Stmt::For` (both the constant-step and dynamic paths — the actual
   collision site was the arm's own unconditional `self.locals.insert(var,
   I64)` pre-existing one line above where the fast/dynamic split occurs),
   `Stmt::ForEach` (pure-Julia iterate path and the builtin split path),
   `Stmt::ForEachTuple`, single-variable comprehensions, and (added after
   advisor review surfaced it as a separate, unwired collision site)
   tuple-destructuring comprehensions (`[expr for (a,b) in iter]`,
   `compile_tuple_destructuring_comprehension` — reachable through a
   different branch than the single-variable case since
   `decode_tuple_comprehension_binding` early-returns before the
   single-variable path's `shadow_local_enter` call; fixed with the same
   `Vec<ShadowedLocal>` pattern used for `Stmt::ForEachTuple`, verified
   confirming neither `compile/inference.rs`'s pre-scan nor the
   abstract-interpretation engine model comprehension-local bindings at
   all — the engine already isolates them via a cloned `body_env` — so no
   matching fix was needed in either file for this construct). The
   runtime-value fix alone was insufficient for the `for`/`foreach`
   statement forms: the whole-function slot-typing pre-scan
   (`compile/inference.rs`) and the abstract-interpretation return-type
   engine (`compile/abstract_interp/engine/mod.rs`) each independently
   modeled the loop's leaked type forward past the loop, producing a wrong
   `Instr::ReturnI64` choice that crashed at runtime even after the
   bytecode-level value was correctly restored — both needed the same
   save/restore treatment. Explicitly decomposed out (new issues):
   multi-generator comprehension (`[i+j for i in R1, j in R2]`) shadowing →
   **#11000**; exception-unwind bypassing the restore's straight-line exit
   convergence point → **#11001** (shared gap with `LetBlock`'s own
   restore, not a #10984-introduced regression). A separately-discovered,
   unrelated crash (catch variable vs. a differently-typed outer local)
   filed as **#10999** — confirmed catch_var's overwrite-not-restore is
   itself upstream-correct, so it needed no shadow/restore change. The
   lazy generator form `(x for x in ...)` (single-variable,
   `compile_generator_expr`) was checked separately and already matches
   upstream — a distinct codegen path from `Expr::Comprehension`, not
   covered by this fix, but not found to need one.

   **Pre-merge hardening (same PR, sibling-loop crash + codex review)**:
   the original compile-time-only "is there a live outer value" classifier
   (`initialized_locals` membership) was unsound in BOTH directions.
   False positive: a sibling (non-nested) loop's zero-iteration residue
   read as a live value → unguarded save load of a never-stored slot →
   `UndefVarError` crash, reproduced live in `channels.jl`'s
   `_wake_all_channel_waiters` (fixture
   `scope/sibling_foreach_same_name_zero_iter_first_10984.jl`); same
   crash shape for a conditionally-initialized outer local
   (`if flag; x = ...; end` then `for x`) in a non-slotized frame
   (`scope/shadow_runtime_guard_10984.jl`). False negative: the
   const-step integer `for` counter registers a `locals` type WITHOUT
   `initialized_locals`, so a nested same-name loop skipped the save and
   clobbered the live outer counter mid-iteration — sjulia `(21, [12])`
   vs julia `(63, [1, 2, 3])` (pre-existing divergence on main, found by
   codex review). Final mechanism: (1) symmetric snapshot/restore of all
   five name-keyed bookkeeping structures on enter/exit
   (`restore_shadow_bookkeeping` — removes entries that were absent
   pre-enter, so no phantom residue survives a construct); (2) the
   runtime save/restore bytecode is bracketed by an `Instr::IsDefined`
   guard with a Bool flag slot — the runtime flag, not compile-time
   bookkeeping, decides whether a value is actually saved/restored;
   (3) the emission gate stays `locals` type entry AND
   `initialized_locals` — the whole-function pre-scan seeds `locals`
   types for EVERY local including each loop's own fresh induction
   variable, so gating on the type entry alone emitted the bracket
   (plus a string-keyed `IsDefined` lookup) at every loop entry in every
   function, and that intermediate version broke 20+ categories/binaries
   in the full `release-fast` sweep (the gate is a correctness
   requirement, not just perf) — instead, each shadowing loop arm
   truthfully inserts `initialized_locals` for its own induction
   variable (the element/counter store dominates every body run), so
   NESTED same-name constructs see `initialized == true` and emit the
   guarded save, while the symmetric restore removes the entry at the
   construct's exit and fresh-name loops stay at zero shadow bytecode
   (verified via `--dump-bytecode`). Residual (codex, medium): a
   guard-false exit restores bookkeeping but cannot un-store the
   construct's own writes to the named runtime slot, so a
   post-construct read that upstream rejects with `UndefVarError` can
   still see a stale value — the pre-existing flat-map leak class,
   unchanged by this PR.

## Cluster B — Static/dynamic macro path desync

Confirmed as a real architectural pattern, and **already substantially
addressed**: PR **#10981** ("techdebt(#10627): converge macro engines'
Pass-1 hygiene decision table onto a shared registry") converges the static
stdlib/Base path (`lowering/expr/quote/`) and the dynamic
VM-backed path (`macro_runtime.rs`) onto one shared `quote_binding_role`
classifier, replacing two independently hand-maintained `match ExprHead`
blocks. It explicitly scopes out Pass-2 codegen convergence
(`Function`/`Where`/`Comprehension`/`Generator`, tracked by **#10916**) and
one destructuring-assignment gap (**#10980**) as follow-ups — matching
exactly the bugs #10983's cluster B cites (#10916, #10926, #10923, #10980).

**Gap in #10983 itself**: its cluster B section lists #10916/#10926/#10923/
#10980 as symptoms but never mentions **#10627**, the actual epic (with an
in-flight PR) driving the structural fix. Recommend adding that link when
#10983 is next edited.

## Cluster C — Token syntactic-operator role

Confirmed. Live MWE:

```julia
f = &&
println(f)
```

| Runtime | Output |
|---|---|
| `julia --startup-file=no` (1.12.6) | `ParseError: invalid identifier` (rejects `&&` as a bare identifier) |
| `target/dev-fast/sjulia` | `function &&` (accepts it as a first-class value) |

Matches #10983's claim that `Token::is_operator()` conflates "participates
in operator grammar" with "permitted as a first-class value." Already
correctly owned and cross-linked: **#10940** ("Prevention: keep parser
syntactic-operator roles exhaustive"), filed the same day and pre-dating
#10983, already lists #10932/#10933 as the representative bugs. Two related
fixes (PR **#10939** for #10917, merged) confirm the pattern is real and
actively being worked, not merely theorized.

## Cluster D — Declaration-statement normalization

Confirmed. PR **#10950** ("fix(parser): normalize scoped const declarations",
merged 2026-07-13T16:43:23Z) fixed two concrete instances the same day
(#10938 `global const` split into a bogus declaration + assignment; #10947
lost RHS-parsing precedence forms) — direct evidence the
declaration-modifier-grammar gap is real and reproducible, not merely
inferred. Already correctly owned and cross-linked: **#10951**
("Prevention: differential scoped-declaration modifier grammar matrix"),
filed the same day, already lists #10937/#10943/#10945 as the residual
symptom issues #10983 also cites.

## Cluster E — Where-binder shadowing in Type annotations

Confirmed as a live symptom, **including the specific mechanism #10983's
text names**. Live MWE re-run on current HEAD:

```julia
f(::Type{Float64}) where Float64 = Float64{Int64}
println(f(Vector) == Vector{Int64})
```

| Runtime | Output |
|---|---|
| `julia --startup-file=no` (1.12.6) | `true` |
| `target/dev-fast/sjulia` @ a891079ef | `Runtime error: MethodError: no method matching f(::Type{Vector})` |

This still reproduces on the current verification HEAD (the original
#10942 report ran against main plus an *uncommitted* #10934 patch; this
confirms the symptom independent of that patch's landing state).

#10983 attributes this to `convert_type_with_type_vars`
(`subset_julia_vm_lowering/src/lowering/function/where_clause.rs:288-321`) being
"`JuliaType::Struct`-limited." An initial read of the function looked like
a counter-example — it *does* recurse into `JuliaType::TypeOf` (i.e.
`Type{...}`, line 298-300) — but the recursion only helps if the inner
type arrives as `JuliaType::Struct(name)`, and for a builtin-spelled name
it never does:
`subset_julia_vm_types/src/types/julia_type/parsing.rs:493` maps the
string `"Float64"` directly to the dedicated enum variant
`JuliaType::Float64`, not `JuliaType::Struct("Float64")`. So
`Type{Float64}` parses to `TypeOf(JuliaType::Float64)`; the `TypeOf` arm
recurses into `convert_type_with_type_vars(JuliaType::Float64, ...)`,
which falls through the `other => other` catch-all (line 319) and returns
`JuliaType::Float64` completely unchanged — the `Struct(name)`-only match
arm (line 290-297), which is the only place a where-binder name gets
turned into `TypeVar`, never fires. This is exactly why the *ordinary*
spelling (`where T`) works in existing coverage — an arbitrary name like
`"T"` has no builtin-variant arm in `parsing.rs` and falls through to
`JuliaType::Struct("T")`, which *does* hit the `Struct(name)` match arm —
while the builtin-spelled `where Float64` does not: the two spellings take
different `JuliaType` representations before `convert_type_with_type_vars`
ever runs, and only one of those representations is covered by the
function's single match arm. #10983's one-line mechanism claim is
correct.

This confirms the same broader pattern as **#10436**
("Milestone 76 where-binder / subtype / type-object bugs stem from
scattered type-level lowering and name-string scoping"), filed **two days
before** #10983, whose design flaw #2 ("post-hoc where-binder rebinding via
tree walk") and flaw #4 ("duplicate lowering paths for where clauses ... in
at least three places") already describe builtin-name-collision-with-binder
symptoms (e.g. #10407, "where-clause parameter name colliding with builtin
type name") structurally identical to #10942's signature-dispatch case.

**Gap in #10983 itself**: its cluster E section tracks only #10942 and
never mentions #10436, despite #10436 already owning this exact
architectural territory at epic scope. Recommend #10942 be folded under
#10436 (or at minimum cross-linked) rather than tracked as an independent
root cause.

## Net decomposition action

Of #10983's five claimed root causes, **four are already fully owned** by
pre-existing, correctly-scoped tracking issues/epics (#10925, #10936 +
#10965, #10940, #10951, #10627 + #10916 + #10980, #10436). The **one**
genuinely unowned architectural gap found by this pass is `CoreCompiler`'s
flat `locals` map lacking a lexical scope stack (cluster A, third
sub-cause) — filed as **#10984**.

#10983's highest remaining value is not new issue-filing but two missing
cross-links in its own text: **cluster B → #10627** and **cluster E →
#10436**. This doc, plus the verification comment on #10983 itself, is
the durable record standing in for the never-committed memory file
#10983 originally cited.

## Related

- Issue #10983 (this verification's subject)
- Issue #10984 (`CoreCompiler` scope stack — fixed, `Fixes #10903`; residuals
  #10999/#11000/#11001)
- PR touching this doc: `techdebt/10983-rootcause-verification`
