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

2. **Run the workflow**, passing the collected numbers and `merge: true`:
   ```
   Workflow({ name: "fix-bug-issues", args: { issues: [<numbers>], merge: true } })
   ```
   It triages into work-items, fixes each in an isolated worktree with a fixture verified against upstream `julia`, adversarially verifies, and **merges each `confirmed` PR to main as soon as it passes** (regular merge, never squash; doc-only conflicts auto-resolved). `concerns`/`rejected`/`blocked` work-items are NOT auto-merged.

3. **Triage the result.** For every work-item with verdict `concerns` or `rejected` (not merged): read the diff and decide — fix-forward, merge manually, or close the PR. Confirmed ones are already on main.

4. **Final full-suite gate** (REQUIRED after parallel merges — the promote-fallback / dispatch-order OOM trap only shows in the full run; CLAUDE.md):
   ```bash
   git -C /Users/terasaki/work/atelierarith/ailujsoi checkout main && git pull --no-edit
   timeout 1800 cargo nextest run --release --no-fail-fast   # never pipe to `tail` alone — note which test failed
   ```
   Treat a failure as a regression ONLY if it does not also fail on the pre-merge `main`; pre-existing failures (e.g. a separately-tracked one) are out of scope — say so explicitly.

5. **Report**: issues closed, PRs merged, any `concerns`/`rejected` left for review, and the full-suite result.

## Notes

- The repo is under heavy parallel development; the workflow reproduces each bug on up-to-date `main` first and skips ones already fixed (status `fixed-no-pr`) — no duplicate PRs.
- New bug issues can appear *during* a run from other work. This skill handles the snapshot taken in step 1; re-run to pick up newcomers.
- Incidental discovery: if you hit a construct that works in upstream `julia` but not sjulia, file an `unsupported-feature` issue (CLAUDE.md), don't silently route around it.
- Engine: `.claude/workflows/fix-bug-issues.js` (`args.issues` required, `args.merge` optional).
