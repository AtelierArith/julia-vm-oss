# ADR: VM-Level Task Continuations (Issues #10269 / #10144)

*Status: Implemented (S1–S4, Issue #10349). Last updated: 2026-07-13.*

## Decision

Adopt **VM-level task continuations by frame-segment switching** (Option A
below) as the target architecture for cooperative multitasking: each `Task`
owns a resumable segment of the VM's flat call-frame stack, and blocking
primitives (`put!` on a full `Channel`, `take!` on an empty one, `wait`,
`yield`) become VM yield points that save the current task's segment and
switch to the next runnable task — all inside the existing single-threaded
dispatch loop, with no OS threads and no native stack switching.

The former Julia-level runnable queue / `pending_puts` approximation is retired.
`tests/fixtures/concurrency/channel_blocking.jl` now pins upstream behavior,
including the #10144 statement order `[1, 0, 99, 2]`.

## Implementation outcome (Issue #10349)

- S1: `VmTaskContext` owns the active frame suffix, operand stack, return IPs,
  handler stack, instruction position, and task-local exception state. Six
  condition-3 VM intrinsics register/schedule/yield/park/wake/query tasks while
  public semantics remain Pure Julia.
- S2: Channel putter/taker FIFO queues park real continuations; buffered-full,
  empty-take, and unbuffered rendezvous no longer use `pending_puts`. Bound and
  do-block producers are live Tasks, and all-blocked state raises a catchable
  deadlock error instead of hanging an app/test process.
- S3: Task/Condition waiters, `notify`, `waitany`/`waitall`, `@sync`, cooperative
  timers, live Channel iteration, and future `errormonitor` reporting all use
  task completion/wakeup.
- S4: every suspended task value/frame/stack root participates in struct-heap
  mark/remap. All seven `run_until_frame_return` consumers share the native
  re-entry floor; suspension there fails catchably before mutation. Normal
  main-task execution tests one predictable task-id branch only at VM exit,
  preserving the existing `hot_paths_benchmark` dispatch-loop guard.

## Context

### Former model (after Issue #8989 / PR #10151, before #10349)

- `@async body` lowers to `Task(thunk); schedule(t); t`
  (`src/lowering/expr/macros/mod.rs::lower_async_macro_expr`).
- `schedule` pushes onto a Julia-level global runnable queue
  (`__sjulia_task_queue__` in `src/julia/base/task.jl`); nothing runs yet.
- Safe points — `wait(t)`, `yield()`, `take!`/`fetch` on an empty channel,
  `put!` on a full channel — pop the queue and run one task **to completion**
  via a plain Julia call (`__sjulia_run_task` → `t.func()`).
- A `put!` that finds the buffer full (after giving queued tasks one chance
  to drain it) appends to `Channel.pending_puts`; a later `take!` drains one
  pending value per take. Values stay FIFO-correct
  (`src/julia/base/channels.jl`).

There is **no mechanism to suspend a running task mid-function**: once
`t.func()` starts, every statement of the body runs before control returns
to the scheduler. That is the root cause recorded in Issue #10269.

### Pre-implementation behavior boundary (verified 2026-07-11)

| # | Pattern | upstream | sjulia today | Class |
|---|---------|----------|--------------|-------|
| 1 | Schedule-before-run: `t = @async push!(r,2); push!(r,1); wait(t)` | `[1, 2]` | `[1, 2]` | OK (#8989) |
| 2 | Empty `take!` drives a scheduled producer | `42` | `42` | OK (#8989) |
| 3 | **#10144 MWE**: producer `put!;put!;push!(r,99)` on `Channel(1)`, consumer interleaves `take!`/`push!` | `[1, 0, 99, 2]` | `[99, 1, 0, 2]` | **Wrong order (silent)** |
| 4 | `put!`-only producer overflowing capacity; consumer takes all | `[1,2,3,4,5]` | `[1,2,3,4,5]` | OK (eager timing unobservable) |
| 5 | Producer with `println` after each `put!` (cap 1), consumer prints takes | interleaved `produced`/`consumed` | all `produced` lines print while the first `consumed` line is mid-write (compounded by the #10351 println-arg-order bug) | **Wrong order (silent)** |
| 6 | Unbuffered `Channel(0)` rendezvous with `@async` producer | `got 7` then `after put` | producer runs eagerly past the rendezvous (`after put` before the value is consumed; output also garbled by #10351) | Wrong order (silent) |
| 7 | Main task blocks on `put!`, consumer is an `@async` task | `[1, 2]` | consumer task fails: its `take!(c)` mis-dispatches to the builtin IOBuffer `take!` (**#10352**); absent that bug it would hit the honest `cannot block` `InvalidStateException` | **Error (honest, wrong error type)** |
| 8 | Do-block producer `Channel(f, sz)` overflowing capacity | values in order (`f` runs lazily as a task) | straight-line `put!` bodies: values in order (`f` runs eagerly, synchronously). `for` inside the do body fails to lower and `Channel{T}(f, sz)` is missing (**#10353**) | OK (values) / Error |
| 9 | Yield granularity: task pushes, `yield()`s; main pushes between | `[task1, main, task2]` | `[task1, task2, main]` | Wrong order (silent) |

(Verified with `julia --startup-file=no` 1.12 vs `target/dev-fast/sjulia`
built from main `f079e3d0f`. Incidental gaps discovered by this probe sweep
were filed per the Discovery Rules: #10351 println/print argument
side-effect order, #10352 `take!(::Channel)` dispatch inside closures,
#10353 do-block producer lowering.)

The failing class of *this* ADR is precisely: **any observable side effect
of a task after a would-block point, interleaved with another task's side
effects**. The channel *values* themselves are never dropped or reordered —
the FIFO `pending_puts` queue guarantees that — but statement-level
interleaving diverges because a task cannot stop mid-body.

### Why the divergence needs a VM change

`put!`/`take!` are pure Julia (`src/julia/base/channels.jl`); the blocking
condition is visible at the Julia level. But *acting* on it — parking the
producer's partially-executed function and resuming it later — requires
saving and restoring the VM frame stack, instruction pointer, and operand
stack mid-function. No Julia-level construct in the subset can express that.

## Decision drivers

- **Single-threaded VM ADR** (`SINGLE_THREADED_VM.md`, Issue #8649): the
  requirement is cooperative multitasking, not parallelism. OS threads and
  preemption are out.
- **No-JIT iOS / WASM targets**: no `ucontext`/`setjmp` native stack
  switching (App Store review risk, unsupported on WASM). Anything we do
  must live inside the interpreter.
- **Upstream-driven compatibility**: upstream Julia implements tasks in the
  runtime (`julia/src/task.c`) with real context switching; the observable
  semantics we must match are statement-level interleaving at yield points.
- **Interpreter advantage**: ordinary Julia calls already execute
  *iteratively* in one flat dispatch loop — `Vm.frames: Vec<Frame>` (heap),
  a parallel `return_ips: Vec<usize>`, one shared operand stack where each
  `Frame` records its `stack_base`, and a single `self.ip`
  (`src/vm/exec/mod.rs::run`, Issue #5969 notes). A "native stack" to switch
  simply does not exist for ordinary calls; the execution state is already
  plain heap data. This is exactly the strength Issue #8989 identified.

## Options considered

### Option A — VM frame-segment continuations (chosen)

Each running task's live state *is* a contiguous suffix of the flat VM
stacks. Suspending = slicing that suffix off; resuming = pushing it back.

- **Task table**: `Vm.tasks: SlotMap<VmTaskId, VmTask>` with
  `VmTask { frames: Vec<Frame>, return_ips: Vec<usize>, stack_segment: Vec<Value>, handler_segment: Vec<HandlerEntry>, resume_ip: usize, state: Runnable|BlockedOn(..)|Done }`.
  The Julia `Task` struct carries the opaque `VmTaskId` so Julia-level code
  (`schedule`, `istaskdone`, …) stays in pure Julia.
- **Task boundary protocol**: starting a task goes through a VM intrinsic
  (not a plain `t.func()` call), so the segment's bottom frame returns to a
  *task-exit* handler instead of a normal caller — a resumed segment must
  never try to "return" into whatever frame happens to be below it at
  resume time.
- **Yield points as intrinsics**: `put!`/`take!`/`wait`/`yield`/`Condition`
  keep their pure-Julia logic and call a single VM intrinsic
  (`__sjulia_task_park(reason)` / `__sjulia_task_switch()`) when they must
  block. The dispatch loop handles the intrinsic at a clean instruction
  boundary: save segment → pick next runnable → restore its segment (or fall
  back to the main task). New blocking primitives then only need to call the
  same intrinsic — the acceptance criterion of Issue #10269 that adding a
  primitive must not rewrite the scheduler.
- **Blocked queues per resource**: `Channel` gains VM-visible wait lists
  (`cond_put` / `cond_take` in upstream terms) so `take!` wakes exactly one
  blocked putter and vice versa, matching `julia/base/channels.jl` shape.

**Constraint — native re-entry floors.** Some builtins re-enter the
interpreter natively: `run_until_frame_return(target_depth)` (used by
`eval`, builtin-initiated calls such as HOF bodies, `show`, the iteration
protocol in `type_ops/iteration.rs`) holds Rust stack frames mid-loop and
installs `eval_dispatch_floor`. Frames at or below such a floor are pinned
by native Rust frames and **cannot be captured**. Rule: a task may suspend
only if no native floor lies above its root frame; otherwise the yield
point raises a clear, catchable "cannot suspend across a native call
boundary" error (upgraded from today's silent reordering). Stage S4 shrinks
this set by moving hot HOF paths onto the flat loop where feasible.

The guard must be **general**, not `eval_dispatch_floor`-specific (codex
review). The real invariant is "no Rust call frame is waiting on a specific
VM frame depth, stack height, or return-value position." `eval` is only one
re-entry; every builtin/HOF/iteration/`show`/`eval` helper that re-enters
the loop and then resumes native logic *after* the Julia call is equally
non-capturable. S1 therefore introduces a single `native_reentry_floor`
(a depth counter/floor raised on *every* native re-entry, not just `eval`)
and forbids suspension above it — `eval_dispatch_floor` becomes one
contributor to that floor.

**Known pitfalls to engineer for** (from reviewing the VM invariants; also
run past a codex adversarial review):

1. *Handler stack rebase* — try/catch handler entries reference frame depths
   and operand-stack heights; the saved `handler_segment` must be rebased
   relative to the segment root and re-rebased on every resume (resume
   height generally differs from suspend height).
2. *`stack_base` rebase* — every saved `Frame.stack_base` is absolute; on
   resume, add the delta between the old and new segment base. Same for any
   depth-keyed side tables (`generated_expr_pending_eval_frames`).
3. *Frame pool / retirement* (Issue #5172) — suspended frames must not be
   retired into the reuse pool; ownership moves to the `VmTask`.
4. *Executable-block fast paths & `next_executable_ip`* — both are
   ip-anchored caches; a task switch must invalidate/refresh them exactly
   like a return does (`refresh_next_executable_ip_from`).
5. *Struct-heap compaction safepoints* — `compact_struct_heap_at_safe_point*`
   currently only walks live frames/stack; suspended segments hold heap
   references and must be registered as compaction roots.
6. *World age & specialization* — a resumed frame keeps its recorded
   `world_age`; method-table changes between suspend and resume must follow
   the same rules as any long-running frame.
7. *Error unwinding across parked tasks* — a `VmError` raised inside a
   resumed segment must unwind only that segment and mark the task failed
   (upstream: task dies, `wait` rethrows `TaskFailedException`), never the
   scheduler's own frames.
8. *Main task as root* — the main program is task 0 with the same protocol;
   `current_task()` becomes VM-backed.

### Option B — CPS-transform task bodies in lowering (rejected)

Lower `@async` bodies (and transitively every function they may call!) into
continuation-passing style so a blocked `put!` returns a continuation. The
transitive requirement is the killer: `put!` is reached through arbitrary
call chains (`foo` → `bar` → `put!`), so *all* code would need the
transform or a dual compilation mode — code-size blowup and a second
semantics to maintain, on targets where code size matters (iOS). A
statement-granular variant (chunking only the `@async` body itself) was also
rejected: it fixes only bodies where `put!` is a top-level statement — an
ad-hoc scope restriction that violates the "General Over Ad-hoc" repo
principle (the #10144 MWE would pass while the same producer wrapped in a
helper function would still silently reorder).

### Option C — native stack switching / OS threads (rejected)

`ucontext`/`setjmp`-style coroutines or one OS thread per task. Violates the
Single-Threaded VM ADR, risky for iOS review, unavailable on WASM
(asyncify would be a WASM-only fork of execution semantics). Also
unnecessary: ordinary calls hold no native stack to switch (see drivers).

### Option D — keep the Julia-level approximation as the end state (rejected)

The `pending_puts` model keeps channel *values* FIFO-correct and covers
put!-only producers and do-block lazy streams, but rows 3/5/6/9 of the
boundary table are silent wrong-output divergences — the worst class under
the repo's bug-vs-unsupported decision rule. It stays only as the *interim*
semantics because removing it before S1 would replace today's value-correct
behavior of rows 4/8 with errors (a regression deliberately pinned by the
Issue #8989 fixtures in PR #10151).

## Interim semantics (until S1/S2 land)

- `put!` on a full channel does not error and does not suspend: the value
  goes to `pending_puts` (FIFO), and **the producer continues executing** —
  side effects after that point run earlier than upstream (rows 3/5/6/9).
- `take!` on an empty channel first drains the runnable queue (running each
  task to completion); if nothing produces a value it throws
  `InvalidStateException("Channel is empty. In cooperative model, cannot block.")`
  (row 7) — an honest error, never a wrong value. (Inside closure/`@async`
  bodies the error is currently masked by the #10352 dispatch bug.)
- Channel values are never dropped or reordered; only statement
  interleaving diverges.
- Pinned by `tests/fixtures/concurrency/channel_blocking.jl` (including a
  KNOWN DIVERGENCE testset for the #10144 MWE) and documented in
  `UNIMPLEMENTED.md`. Issue #10144 remains open, blocked on S1.

## Staged plan

- **S1a — continuation core**: VM task table, task-boundary start protocol,
  segment suspend/resume (pitfalls 1–4, 7), park/switch intrinsics, general
  `native_reentry_floor` guard with the clear error; wire `yield` and
  `wait`. Acceptance: `yield()` granularity (row 9) matches upstream;
  single-thread invariant test (no `Send`/`Sync` added, one host thread).
- **S1b — buffered channel blocking**: per-channel blocked-putter /
  blocked-taker queues carrying the suspended continuation; wire
  `put!`-on-full and `take!`-on-empty to park/wake; **remove `pending_puts`
  for buffered channels**. Acceptance: #10144 MWE prints `[1, 0, 99, 2]`;
  rows 3/5 match upstream; close #10144. (Codex review: S1 cannot claim to
  fix #10144 while leaving the `pending_puts` overflow path intact — making
  a full `put!` correct *is* the removal of that path, so it is split out of
  S2 into S1b rather than deferred.)
- **S2 — channel semantics parity**: unbuffered rendezvous (row 6), remove
  the remaining `take!` "cannot block" error and unbuffered `pending_puts`,
  `Condition`/`notify`, `bind`/do-block producer constructor on real tasks,
  and an "all tasks blocked" deadlock error (upstream hangs; a clear error
  keeps CI and the iOS app safe — documented divergence).
- **S3 — scheduler surface**: `waitany`/`waitall`, `@sync` on real task
  sets, `sleep` as a yield point, `Channel` iteration driving a live
  producer task, `errormonitor`.
- **S4 — native-floor hardening**: audit `run_until_frame_return` call
  sites (HOFs, `eval`, `show`, iteration protocol); move hot HOF paths onto
  the flat loop or document them as non-suspendable; compaction-root
  integration (pitfall 5) stress tests; perf guard (dispatch-loop overhead
  when zero tasks exist must be one predictable branch).

## Test strategy (from Issue #10269, extended)

- Bounded-channel producer/consumer statement-ordering fixtures (rows 3/5)
  verified against upstream via `scripts/fixture_julia_parity.sh`.
- `@async` + `yield` + `wait` dependency-graph ordering (row 9).
- Nested/recursive `put!`/`take!` (producer that is also a consumer).
- Suspend-under-native-floor: `put!` blocking inside a Rust-driven HOF body
  must raise the S1 error, not corrupt frames (negative test).
- Single-thread invariant: scheduler runs entirely on one host thread;
  no new `Send`/`Sync` bounds (`SINGLE_THREADED_VM.md` audit).
- Full-suite runs after each stage (scheduler bugs are ordering/state
  dependent — see the Issue #5966 lesson).

## Adversarial review (codex, gpt-5.5, 2026-07-11)

An independent codex review of this design (the "design pivot with unclear
consequences" trigger) endorsed Option A as "the right family of solution"
and agreed the interim `pending_puts` approximation is "defensible" as an
explicitly tracked divergence — but not "semantically safe": silent
reordering "violates causality, not just timing" when non-channel side
effects (init, logging, mutation, callbacks) sit after the blocked `put!`.
It confirmed the mitigation (known-divergence bucket, docstrings tied to
#10144, loud debt) and suggested a later strict/debug mode where a full
`put!` without suspension errors instead of overflowing.

Two findings changed this document:

1. **The native-floor guard must be general**, not `eval_dispatch_floor`-
   specific — folded into the S1 constraint above (`native_reentry_floor`).
   The true invariant is "no Rust frame is waiting on a specific VM depth /
   stack height / return-value slot", of which `eval` is one instance.
2. **S1 is not cleanly separable from removing buffered `pending_puts`** —
   making a full `put!` correct *requires* per-channel blocked-putter/taker
   queues holding the suspended continuation plus `take!` wakeup, which *is*
   the removal of `pending_puts` for buffered channels. Reflected in the
   S1a/S1b split above; #10144 closes at S1b, not a hypothetical
   continuations-only S1.

Findings already captured as pitfalls (confirmed, not new): suspended
segments must be full compaction/GC roots (pitfall 5 — codex ranked this the
top blocker); handler-stack rebasing includes stack-height/finally state
(pitfall 1); the task-root sentinel return protocol (pitfall on
`return_ips`); `generated_expr_pending_eval_frames` is depth-keyed and
hostile to suspension (start by forbidding suspension while it has entries in
the segment); frame-pool ownership transfer; per-frame `world_age`
preservation on resume; unifying yield points with compaction safepoints.

## Consequences

- Positive: statement-level interleaving parity for tasks/channels; the
  structural blocker for package producer/consumer patterns (North Star)
  gets a credible path; blocking primitives become one-intrinsic additions.
- Negative/costs: the dispatch loop gains a scheduler dimension touching
  several delicate invariants (handler floors, compaction, frame pool);
  suspended segments pin memory; native-floor cases surface as new (honest)
  errors where today code silently mis-orders.
- Neutral: `Task`/`Channel` stay pure Julia at the surface; only the
  blocking edges call intrinsics — consistent with the Pure Julia First
  principle.

## Related

- Issue #10269 (this design), #10144 (bounded `put!` mid-function suspend,
  blocked on S1), #8989 (runnable queue, closed), #3439/#3451 (historical
  scope-outs), PR #10151 (interim scheduler + fixtures).
- `SINGLE_THREADED_VM.md`, `ARCHITECTURE_OVERVIEW.md`,
  `memory/project/project_8989_task_channel_continuation_boundary.md`.
