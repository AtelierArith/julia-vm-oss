---
name: sjulia-lead-review-merge
description: >-
  Use when acting as the lead/orchestrator (管理者) for parallel implementation
  agents in this repo: an agent has opened a PR and reported completion, and
  you must review its diff, resolve merge conflicts with main, run the local
  gates (GitHub Actions is disabled — local checks are the ONLY gates), and
  drive the PR to a regular merge. Also applies when integrating several
  agent PRs in sequence.
---

# Lead review & merge of an implementation-agent PR

You are the merge gate. GitHub Actions is intentionally disabled on this repo
(budget decision, 2026-07-06 — zero status checks on every PR), so nothing is
verified unless YOU verify it locally. Never merge on the strength of the
agent's report alone.

## Inputs

- The PR number/branch and the linked Issue(s).
- The agent's completion report (claims to verify, not facts).

## 0. Verify from a dedicated lead worktree — never the main checkout

Do ALL lead verification (merges, builds, test runs) in your own worktree:

```bash
git worktree add .claude/worktrees/lead-review origin/main
```

A misbehaving agent can run `git checkout` in the shared main checkout and
yank your branch mid-verification (it happened: results suddenly "regressed"
because the tree under the running check had silently moved to main). When a
result looks impossible, `git reflog` the checkout you ran it in before
debugging the code. Share the warm build cache across worktrees with
`CARGO_TARGET_DIR=<main-checkout>/target` — but never run two cargo
invocations against it concurrently.

## 1. Inspect the diff without checking out the agent's branch

The authoring agent's worktree usually still holds the branch checked out —
`git checkout <branch>` and local `git branch -D` will fail. Review from
`origin` instead:

```bash
git fetch origin <branch>
git diff --stat main...origin/<branch>
git diff main...origin/<branch> -- <files-of-interest>
git show origin/<branch>:<new-file>          # read new files in full
```

Review checklist:

- **Scope**: every changed file is explained by the Issue. No unrelated
  resets, no `target/`, editor droppings, or another agent's files.
- **Hard rules** (CLAUDE.md "Hard Rules — Quick Check"): workarounds carry an
  `(Issue #NNNN)` comment + WORKAROUNDS.md entry; fixtures end with `true`,
  are registered in `manifest.toml`, category-prefixed; sample-body edits
  applied to ALL surfaces (`bash scripts/check_sample_body_consistency.sh`);
  STATUS.md/DONE.md additions go UNDER the existing daily header as `###`
  subsections, not as a new top block.
- **Claims vs evidence**: pick the PR's central claim and reproduce it
  locally (run the new script, the new fixture, the measured command). For a
  new/changed audit script, ALWAYS run a negative test: inject a genuine
  violation → the audit must FAIL naming the cause → restore → green again.
- **Report red flags**: "should work", missing exact test counts, a full
  suite the agent "ran" but reports no numbers for. Ask the agent (or rerun)
  before merging.

**Pin the head SHA before final verification.** Agents may push more commits
AFTER reporting completion (a late bug-fix commit invalidated one verified
suite run). Re-fetch, record the PR head SHA, and get the agent's explicit
"branch final at <sha>" confirmation before the expensive final gates.

## 2. Resolve conflicts on a review branch

```bash
gh pr view <N> --json mergeable,mergeStateStatus
# if CONFLICTING:
git checkout -b review/<issue>-merge origin/<branch>
git merge origin/main          # never rebase/force-push an agent's branch
```

Union-resolution rules for the common both-append conflicts:

- **DONE.md / STATUS.md**: keep BOTH `### ... (Issue #NNNN)` subsections
  under the ONE shared daily `## 最新対応 (...)` header (Issue #3760 policy).
- **fixtures manifest.toml**: keep BOTH `[[tests]]` blocks and re-count the
  headers afterwards — a union-resolution once dropped a `[[tests]]` line and
  silently deregistered an entire category (#9378 bigfloat incident).
- **ci.yml audit lists**: keep both shellcheck lines / run steps.
- Never discard a hunk silently; if you must drop something, say why in the
  merge commit body.
- After editing: verify no markers remain with `! grep -rn '<<<<<<<' <files>`.
  NEVER use `grep -c` as a success check inside a `&&` chain — it exits 1 on
  zero matches, silently aborting the rest of the chain (a merge commit once
  went un-committed and un-pushed this way while later commands "succeeded").
- **Code files that auto-merge cleanly can still be wrong**: two branches
  appending adjacent code stack both versions (a shadowed `let` binding), and
  a union-resolved struct-field hunk can splice a field initializer into the
  middle of an unrelated method. Always `cargo check` immediately after a
  cross-PR resolution and treat any `unused_variable` warning as a merge
  artifact until proven otherwise.

**Batching sibling VM PRs.** Two VM-touching PRs pending at once → build ONE
review branch (main + A + B), fix cross-PR integration there, run ONE full
suite. Push the combined branch to PR A's head and merge A — GitHub then
auto-marks PR B merged (its commits are contained), and both `Fixes #` links
close. The de-facto last merge owns the suite either way.

## 3. Verify on the merged tree (the local gate)

**Use the guarded wrapper for the final run** (Issue #9644 — a clippy
warning once landed on main because the final gate wasn't rerun after
`main` advanced during a parallel merge window):

```bash
bash scripts/premerge_gate.sh                       # freshness + clippy + freshness re-check
bash scripts/premerge_gate.sh --merge-main --nextest 'fixture_tests <category>::'
bash scripts/premerge_gate.sh --full-suite          # + full release nextest (last of a batch)
```

It fetches `origin/main`, refuses to run on a branch that doesn't contain
the exact current `origin/main` SHA (`--merge-main` merges it in first),
runs the gates, then re-fetches: if `origin/main` moved DURING the gates it
fails — that verification is stale, merge the new main and rerun. On green
it prints the certified HEAD SHA for a pinned merge.

Scale to the diff — these run on the review branch AFTER the merge commit:

- Affected `scripts/check_*.sh` / `audit_*.sh`, plus the negative test for
  any new audit.
- Rust changes: `cargo fmt --check` on touched files +
  `timeout 1800 cargo clippy --all-targets -- -D warnings`.
- Narrow changes: the relevant category —
  `timeout 1800 cargo nextest run --release --test fixture_tests <category>::`
- VM / compiler / dispatch / inference / test-harness changes, and the LAST
  merge of a parallel batch: FULL suite —
  `timeout 1800 cargo nextest run --release --no-fail-fast` (never `| tail`).
- AoT-touching diffs: `bash scripts/test_aot.sh`.

## 4. Land it

Implementation agents never mark their PRs ready and never merge them. Require
the PR to remain draft throughout review; lead certification is the only
authority for the ready transition (Issue #11056).

```bash
git push origin HEAD:<branch>     # push the resolution to the PR branch
gh pr view <N> --json mergeable,mergeStateStatus   # recompute lags a few s — retry
bash scripts/premerge_gate.sh --pr <N>  # certify draft, mark ready, pinned regular merge
```

- `--pr <N>` checks that the PR is OPEN, draft, targets the configured base,
  and points at local HEAD both before and after the gates. It marks ready only
  after the final freshness check, publishes the server-required
  `sjulia/guarded-certification` status, then pins the regular merge to that
  SHA. A late push or manual ready action makes the gate fail loudly, while
  the strict GitHub ruleset rejects stale or uncertified heads (Issue #11087).

- Do not fall back to a direct or auto merge when guarded certification fails.
  On "Base branch was modified" / "not mergeable", keep or return the PR to
  draft, fetch and merge `origin/main` into the review branch, push, then rerun
  the full guarded command; GitHub state recompute can lag a few seconds.
- `--delete-branch` may fail with "used by worktree" when the agent's
  worktree still holds the branch — harmless; skip local deletion and let
  worktree cleanup handle it.
- Other sessions merge to main concurrently: a CLEAN status can flip to
  CONFLICTING between your check and the merge call. Loop: fetch → merge
  origin/main into the review branch → resolve → push → retry. And always
  diff against a freshly fetched `origin/main`, not the local `main` ref —
  a stale local `main` makes a 8-file PR look like a 125-file one.
- Machine discipline across parallel agents: only ONE full suite runs on the
  machine at a time (the lead's); tell implementation agents to skip their
  own full-suite runs, and to never kill processes by name pattern (`pkill`)
  — an agent's broad kill once SIGTERM'd the lead's verification suite.

## 5. After the merge

```bash
git checkout main && git pull
git branch -D review/<issue>-merge
gh issue view <issue> --json state    # confirm Fixes #NNNN auto-closed it
```

- Unblock dependent work (wave-2 tasks waiting on this merge).
- **Cross-PR integration**: when sibling agent PRs land in sequence, re-run
  the checks that couple them on main (e.g. a new `check_*.sh` from one PR
  vs an audit-coverage framework from another; sample-body audit vs sample
  migrations). The last merger owns the full-suite run on main.
- Feed fixes back: if the review found defects, prefer `SendMessage` to the
  authoring agent (it has the context) with a concrete fix list and re-review
  the delta. Fix directly only trivial nits and conflict resolutions — and
  add commits on top; never rewrite the agent's history.
