# Root-cause quality prevention plan

Status: **accepted design authority** (Issue #10452, 2026-07-14).

This document turns the 2026-07-04 through 2026-07-11 survey of 403
`bug`-labelled Issues into stable ownership and verification rules. It is not a
claim that every listed implementation epic is complete. The parent analysis
owns the classification, priorities, metrics, and handoff; each open child
Issue owns its production migration and remains open until its own acceptance
criteria pass.

## What the baseline means

The original GitHub query was:

```text
repo:AtelierArith/ailujsoi label:bug created:2026-07-04..2026-07-11 state:all
```

It returned 403 Issues. Of those, 228 closed within 24 hours and 61 had
prevention, design, audit, or tech-debt shaped titles. The count therefore
mixes defect discovery, proactive audits, and structural follow-up work. It is
not a production incident rate and must not be optimized downward by reporting
fewer gaps.

Use recurrence within a root-cause class, silent wrong results, equivalent-lane
divergences, open age, and main-red duration as the quality signals. The frozen
four-window baseline is `QUALITY_WEEKLY_BASELINE_2026_07_11.md`; regenerate it
with `scripts/quality_issue_report.py weekly`. North-Star NS-6 remains the
authority for full-suite gate health.

## Canonical root-cause ownership

The original query's label membership is mutable: rerunning it after later
label edits no longer returns 403. `scripts/quality_issue_report.py triage`
reconstructs membership at Issue #10452's creation timestamp by replaying label
events. Its reviewed 403-row output is frozen in
`QUALITY_ROOT_CAUSE_TRIAGE_2026_07_11.tsv`. Every row has one class/owner/reason;
class 0 deliberately leaves an unmatched local symptom owned by its own Issue
instead of forcing it into an unrelated architecture epic.

The title/reference classifier only proposes an initial disposition. The
committed TSV is the review authority: `--reviewed-from` preserves its canonical
class/owner/reason fields while regenerating historical membership and state,
so later Issue title/body edits cannot silently reclassify the survey.

```bash
python3 scripts/quality_issue_report.py triage \
  --created 2026-07-04..2026-07-11 \
  --as-of 2026-07-11T02:59:44Z --expect 403 \
  --reviewed-from docs/vm/QUALITY_ROOT_CAUSE_TRIAGE_2026_07_11.tsv
python3 scripts/quality_issue_report.py weekly \
  --window 2026-06-14..2026-06-20 \
  --window 2026-06-21..2026-06-27 \
  --window 2026-06-28..2026-07-04 \
  --window 2026-07-05..2026-07-11 \
  --reviewed-from docs/vm/QUALITY_ROOT_CAUSE_TRIAGE_2026_07_11.tsv
```

The row-level artifact uses this canonical taxonomy:

| Class | Semantic fault line | Production owner | Completion evidence |
|---|---|---|---|
| 1 | Bare-name identity instead of owner-scoped identity | #10459 and its migration Issues #10988–#10992, #11032, #11078, #11095 | `SEMANTIC_IDENTITIES.md`, zero-growth/retirement ratchets |
| 2 | Loss of structured UnionAll / TypeVar semantics | #10460 | `TYPE_REPRESENTATIONS.md`, structured carriers, differential type tests |
| 3 | Multiple call-semantic implementations | #10461 | one resolver boundary plus direct/callable/HOF/specialized parity |
| 4 | Partial compile-context cache restoration | #10462 | `COMPILE_CONTEXT_REHYDRATION.md`, fresh/restored semantic snapshot equality |
| 5 | Ad hoc iterator traits and consumer-specific protocols | #10463 | trait algebra and protocol-driven consumer matrix |
| 6 | Lowering loses expression value or source intent | #10464 | value-preserving IR and lowering-entry differential tests |
| 7 | Missing equivalent-lane and gate ownership | #10465 (complete) | five-lane `scripts/metamorphic_equivalence.sh` gate |
| 8 | Exception type/layer/catchability drift | #10813, implementation #11146–#11148 | exception taxonomy, catchability parity, enforcement ratchet |
| 9 | Non-transactional typed-loop bailout | #10814 | derived effect metadata and generic/optimized parity |
| 10 | AoT independently reimplements VM semantics | #10815 | expanded VM/AoT differential coverage |

When a bug fits multiple rows, select the earliest semantic owner that lost the
information; downstream symptoms may link secondary owners but must not spawn a
new competing architecture epic.

## P0 differential safety net

Issue #10465 delivered a manifest-driven equivalence harness covering more than
the four original P0 lanes:

- direct / qualified / first-class / higher-order callable;
- Main / generated module wrapper;
- fresh / isolated primed-and-restored cache;
- generic / optimized SSA pipeline; and
- VM / all three AoT acceptance kernels (coprime pi, Aizawa, Mandelbrot).

The harness compares value, result type, and exception class. Known
divergences are Issue-linked and two-sided: an unregistered divergence fails,
and a registered divergence that later agrees also fails as stale. Use:

```bash
bash scripts/metamorphic_equivalence.sh --selftest
bash scripts/metamorphic_equivalence.sh
bash scripts/premerge_gate.sh --metamorphic
```

`premerge_gate.sh` also selects the harness automatically when a branch changes
compiler, lowering, VM, pure-Julia Base, parser, runtime-type/SSA, type-system,
bytecode, AoT/runtime, or equivalence-corpus paths. Each program execution is
bounded by `SJULIA_METAMORPHIC_CASE_TIMEOUT` (120 seconds by default), and a
timeout fails closed even if both lanes hang. `--metamorphic` remains the
explicit force option. The source-audit sync check has positive controls across
Rust compiler, pure Julia, runtime types, and parser paths plus a docs-only
negative control.

This gate detects lane drift; it does not replace upstream Julia fixture parity
or prove the open structural migrations complete.

## Weekly metrics contract

Report a dated window and the exact query/command for every value. Never compare
raw `bug` totals without also separating `prevention`/`tech-debt` work and
rapid-close audit bursts.

| Signal | Definition | Desired direction | Owner / evidence |
|---|---|---|---|
| Root-cause recurrence | New `bug` Issues assigned to an already-known class during the window | down after that class migrates | class table above; link the owner Issue in each new report |
| Silent wrong results | Bugs where sjulia completes with a wrong value/type/identity instead of rejecting | zero escaped; every case gets a differential invariant | bug Issue + fixture + equivalence corpus where a lane exists |
| Equivalent-lane divergence | New or still-allowlisted mismatches in the five lane groups | zero unregistered; allowlist down | `EQUIVALENCE_KNOWN_DIVERGENCES.tsv` |
| Open age | Age of open `bug` Issues, reported by median and oldest | down without suppressing discovery | GitHub `createdAt` / current state |
| Main-red duration | Time from the first reproducible failure report until the fixing merge reaches `main` | down | fixing PR merge timestamp; the frozen baseline only records the clearly named Issue-close proxy |
| Gate health | Full-suite pass/fail, test count, duration, and source-audit result | zero failures | North-Star NS-6 and guarded certification |

The committed baseline contains four comparable full UTC windows. It shows the
July audit campaign increased both discovery and rapid closure, so it makes no
improvement claim from raw volume. Later reports must regenerate full windows
with the same collector and interpret campaign-driven discovery separately.

## Acceptance ledger and handoff

| Original #10452 criterion | State at design close | Continuing owner |
|---|---|---|
| Classify the 403-Issue survey into canonical root causes | complete: historical 403-row TSV with reviewed, frozen class/owner/reason fields plus taxonomy above | new bugs link the matching owner |
| Put cache, name, callable, and Main/module differential lanes in guarded premerge | complete: relevant paths auto-select the five-group harness; VM/AoT covers all three acceptance kernels | `premerge_gate.sh` routing controls and equivalence corpus |
| Explain every fresh/restored structural snapshot difference | implementation open | #10462 |
| Approve owner-scoped identity architecture and phased rollout | design complete; production table retirement continues | #10459 descendants listed above |
| Measure four comparable weekly windows and report improvement honestly | complete: four-window frozen baseline; campaign mix means no raw-volume improvement claim | `quality_issue_report.py weekly` and future regenerated records |
| Add each new silent-wrong bug to a metamorphic/differential invariant | policy and gate complete; case-by-case enforcement continues | bug fixer plus the relevant class owner |

Closing #10452 means the analysis has one durable plan and every unfinished
behavior has one open owner. It does **not** close or supersede #10460–#10464,
#10813–#10815, their implementation children, or any symptom bug.
