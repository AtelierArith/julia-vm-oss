---
name: fix-bug-issues
description: Use when asked to clear, triage, fix, or resolve the open `bug`-labeled GitHub issue backlog for this repo (SubsetJuliaVM / AtelierArith/ailujsoi) — collect all open bug issues, fix each in parallel git worktrees, open PRs, and merge.
---

# Fix Bug Issues (collect → parallel fix → PR → merge)

## Overview

Clears the open `bug` backlog end to end. A multi-agent workflow does the heavy lifting (cluster → fix in isolated git worktrees → adversarially verify → merge each confirmed PR **independently, without waiting for siblings**). This skill wraps it with the two steps the workflow cannot do itself: **collecting the live issue list** (`gh`) and the **final full-suite gate** (`cargo nextest`).

The workflow is opt-in multi-agent orchestration; invoking this skill IS the user's opt-in.

## Steps

1. **Collect the LIVE list** (never a hardcoded snapshot — stale snapshots have fixed already-closed issues):
   ```bash
   gh issue list --repo AtelierArith/ailujsoi --label bug --state open --limit 100 --json number --jq '[.[].number]'
   ```
   If empty, report "no open bug issues" and stop.

2. **Run the workflow**, passing the collected numbers with `merge: false` so
   implementation PRs remain draft for lead certification:
   ```
   Workflow({ name: "fix-bug-issues", args: { issues: [<numbers>], merge: false } })
   ```
   It triages into work-items, fixes each in an isolated worktree with a fixture
   verified against upstream `julia`, opens each implementation PR as draft,
   and adversarially verifies it. Implementation agents never mark ready or
   merge (Issue #11056).

3. **Lead-certify confirmed PRs sequentially.** For every `confirmed` result,
   review the exact diff, update it to current `origin/main`, run the required
   guarded gate (full suite for VM/compiler/dispatch/inference changes), then
   land it with `bash scripts/premerge_gate.sh --pr <N>`. Keep
   `concerns`/`rejected` PRs draft for fix-forward or close them; never merge
   them manually around the gate.

4. **Final full-suite gate** (REQUIRED after parallel merges — the promote-fallback / dispatch-order OOM trap only shows in the full run; AGENTS.md):
   ```bash
   # from the repository root
   git checkout main && git pull --no-edit
   timeout 1800 cargo nextest run --release --no-fail-fast   # never pipe to `tail` alone — note which test failed
   ```
   Treat a failure as a regression ONLY if it does not also fail on the pre-merge `main`; pre-existing failures (e.g. a separately-tracked one) are out of scope — say so explicitly.

5. **Report**: issues closed, PRs merged, any `concerns`/`rejected` left for review, and the full-suite result.

6. **Post-mortem** (`sjulia-postmortem`): record run-level insights in `./memory/` (clusters that recurred, verification gaps), and file follow-up Issues for anything deferred.

## Notes

- The repo is under heavy parallel development; the workflow reproduces each bug on up-to-date `main` first and skips ones already fixed (status `fixed-no-pr`) — no duplicate PRs.
- New bug issues can appear *during* a run from other work. This skill handles the snapshot taken in step 1; re-run to pick up newcomers.
- Incidental discovery: if you hit a construct that works in upstream `julia` but not sjulia, file an `unsupported-feature` issue (AGENTS.md Discovery Rule), don't silently route around it.
- Engine: `.claude/workflows/fix-bug-issues.js` (`args.issues` required; returns
  verified draft PRs for the lead to certify sequentially).
- **Requires the Claude Code `Workflow` tool** (step 2). Agents without it
  (Codex, opencode, Cursor, …) fall back to a sequential loop: for each issue,
  reproduce on up-to-date `main` first (skip if already fixed), fix it in its
  own worktree/branch following `sjulia-dev`, verify the fixture against
  upstream `julia`, then PR + merge via `sjulia-finish-branch` — and still run
  step 4's full-suite gate at the end.
