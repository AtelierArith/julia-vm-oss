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
- **Regular merge only.** Never squash. Use `gh pr merge --auto --merge`.
- **Don't discard others' work.** No `reset --hard`, `clean -fdx`, branch
  deletion, force-push, or history rewrite on files you didn't modify without
  explicit approval.

## Workflow

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
ordering. Use `sjulia-dev/pr-flow` for the full PR body template, required
builds, and post-PR doc-update policy. Each commit message body must explain
WHY and reference `Issue #NNNN` (`Fixes #NNNN` only on the final commit if it
closes the issue).

### 4. Verify

After the final commit, run the relevant checks for the touched areas:

```bash
cargo build --release --bin sjulia --features repl
timeout 1800 cargo nextest run --release --test fixture_tests <category>::
```

Run the full suite before merge when VM/compiler/runtime changed:

```bash
timeout 1800 cargo nextest run --release
```

### 5. Push and open PR

```bash
git push -u origin <branch>
gh pr create --title "..." --body "$(cat <<'EOF'
## Summary
- <what changed and why>

## Test plan
- [ ] <tests run>

Linked Issue: #NNNN
EOF
)"
```

### 6. Merge

```bash
gh pr merge --auto --merge
```

If `--auto` is unavailable, merge manually only after required checks pass.

### 7. Clean up

```bash
git checkout main && git pull origin main
```

## Anti-patterns

- One giant commit bundling unrelated changes.
- `git add .` or including someone else's uncommitted work.
- Squash merge (`gh pr merge --squash`).
- Merging manually just because `--auto` is unavailable; confirm required checks
  are green first.
- Merging before required builds/tests complete.
- PR body that only restates the diff.

## Red flags — STOP and re-read this skill

- "Checks passed locally, so I can merge immediately."
- "I'll use squash because the PR has many commits."
- "There are unrelated changes, but they're small so I'll include them."
- "Auto-merge isn't available, so I'll merge by hand right now."
