---
name: sjulia-finish-branch
description: >-
  Use when finishing a SubsetJuliaVM (sjulia) branch and need to turn its
  changes into logical commits, open a pull request, and merge it in one
  continuous flow. Applies after a multi-file change is complete and the next
  step is commit → PR → merge.
---

# Finish a sjulia branch (commit → PR → merge)

Take a working branch from diff to merged PR as a single, coherent workflow.
Preserve logical commit boundaries, keep unrelated changes out of the PR, and
follow the repo's regular-merge policy.

## Hard rules

- **One logical unit per commit.** Split independent concerns; keep dependent
  files together (fix + regression test + its docs entry).
- **Stage named files/hunks only.** Never `git add .` / `git add -A`.
- **Leave strangers behind.** If a file is modified but does not belong to your
  change, leave it uncommitted and warn the user.
- **Never `git stash`.** The stash is shared repo-globally across sessions and
  worktrees — you can clobber or pop another agent's WIP. Park work on a
  temporary branch commit instead.
- **Regular merge only.** Never squash. The lead uses
  `scripts/premerge_gate.sh --pr <N>` to certify, mark ready, and merge.
- **Don't discard others' work.** No `reset --hard`, `clean -fdx`, branch
  deletion, force-push, or history rewrite on files you didn't modify without
  explicit approval.

## Workflow

### 0. Confirm you are on YOUR branch

Shared worktrees get their HEAD switched by other agents. Check now, and again
immediately before every commit:

```bash
git branch --show-current
```

- Your feature branch → continue.
- `main` or an unfamiliar branch → STOP. Check out your own branch (create it
  from `main` if it doesn't exist yet). Never commit onto `main` or someone
  else's branch; never reset refs you don't own.

### 1. Inspect

```bash
git status
git diff --stat
git diff
```

Identify logical units and any unrelated changes.

### 2. Plan

List the commits in order (foundations first): subject, files/hunks, WHY, Issue
number. Get user approval when the split is non-trivial.

### 3. Commit each unit

Use `sjulia-logical-commits` for hunk/file staging, message format, and
ordering. Use `sjulia-dev/pr-flow.md` for the full PR body template, required
builds, and post-PR doc-update policy. Each commit message body must explain
WHY and reference `Issue #NNNN` (`Fixes #NNNN` only on the final commit if it
closes the issue). Re-run `git branch --show-current` before each `git commit`.

### 4. Verify

After the final commit, run the gates for the touched areas:

```bash
# Rust touched → format + lint gates (clippy passing does NOT imply fmt-clean)
cargo fmt --check          # fix with: rustfmt --edition 2021 <only files you touched>
cargo clippy --all-targets -- -D warnings

# Build + category tests
cargo build --release -p subset_julia_vm --bin sjulia --features repl
timeout 1800 cargo nextest run --release --test fixture_tests <category>::
```

Run the FULL suite (all binaries — fixtures + `--lib` alone miss integration
binaries) before merge when VM/compiler/dispatch/inference changed:

```bash
timeout 1800 cargo nextest run --release
```

### 5. Push and open PR

```bash
git push -u origin <branch>
gh pr create --draft --title "..." --body "$(cat <<'EOF'
## Summary
- <what changed and why>

## Test plan
- [ ] <tests run>

Linked Issue: #NNNN
EOF
)"
```

The author leaves the implementation PR draft. It must not be marked ready or
merged until the lead has completed review and guarded verification.

### 6. Lead certification and merge

```bash
bash scripts/premerge_gate.sh --pr <N>
```

Only the successful exact-main/exact-head guarded run marks the PR ready and
performs the pinned regular merge (Issue #11056).

### 7. Clean up and post-mortem

```bash
git checkout main && git pull origin main
git branch -d <branch>
```

Then run the post-mortem (`sjulia-postmortem`): record insights in `./memory/`,
file the prevention Issue if this was a bug fix, file follow-up Issues for
deferred work.

## Anti-patterns

- One giant commit bundling unrelated changes.
- `git add .` or including someone else's uncommitted work.
- Squash merge (`gh pr merge --squash`).
- Merging manually around `premerge_gate.sh --pr <N>`; guarded certification
  owns readiness and merge.
- Merging before required builds/tests complete.
- PR body that only restates the diff.
- Committing without re-checking the current branch in a shared worktree.

## Red flags — STOP and re-read this skill

- "Checks passed locally, so I can merge immediately."
- "I'll use squash because the PR has many commits."
- "There are unrelated changes, but they're small so I'll include them."
- "Auto-merge isn't available, so I'll merge by hand right now."
- "I'm surely still on my branch." — You checked hours ago; check again now.
- "I'll stash this for a moment." — The stash is shared; use a branch commit.
