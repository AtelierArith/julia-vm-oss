---
name: resolve-pr-reviews-and-merge
description: >-
  Use when asked to merge a GitHub pull request and there may be open review
  comments, or when a PR has unresolved review threads that must be addressed
  before merging. Applies when the merge is blocked by reviewer feedback, by
  requested changes, or by open comment threads that need resolution.
---

# Resolve PR review comments and merge

Handle open GitHub review comments before merging: inspect them, make required
code changes, mark threads resolved, and merge only after every actionable
thread is closed.

## Core principle

**Code changes alone do not close review threads.** A fixed comment stays
unresolved on GitHub until it is explicitly marked resolved. Verify with the
GitHub API/UI, not by assuming the fix is enough.

## When to use

- A user asks you to merge a PR that has open review comments.
- A PR shows review feedback you need to address before landing.
- A reviewer requested changes and you are now ready to resolve them.

When NOT to use:

- The PR has no open comments and is already mergeable — just merge it.
- You are the lead reviewing someone else's PR — use `sjulia-lead-review-merge`
  instead.

## Workflow

### 1. Inspect open review threads

```bash
gh pr view <N> --json reviewDecision,mergeStateStatus,reviews,comments
gh pr diff <N>
```

Also list unresolved review threads. Get the owner and repo from `gh`, then use
GraphQL:

```bash
OWNER_REPO=$(gh repo view --json owner,name -q '.owner.login + "/" + .name')
OWNER=${OWNER_REPO%/*}
REPO=${OWNER_REPO#*/}
gh api graphql -F owner="$OWNER" -F repo="$REPO" -F pr=<N> -f query='
query($owner:String!, $repo:String!, $pr:Int!) {
  repository(owner:$owner, name:$repo) {
    pullRequest(number:$pr) {
      reviewThreads(first:100) {
        nodes { id isResolved path originalLine comments(first:1) { nodes { author { login } body } } }
      }
    }
  }
}'
```

Count threads where `isResolved` is `false`. If there are none, skip to step 6.

### 2. Classify each comment

For each open thread decide:

| Type | Action |
|------|--------|
| Requested code change | Make the change, reply, resolve. |
| Question / clarification | Reply with an answer; resolve if the reviewer is satisfied. |
| Optional suggestion (nit) | Either accept the change or reply explaining why not; resolve. |
| Outdated / already fixed | Reply noting it is addressed; resolve. |

Do NOT resolve a thread without either making the requested change or replying
to explain why not.

### 3. Check out the PR branch safely

Before switching branches, verify your current worktree state — especially in a
shared or multi-agent worktree where switching away would disturb someone else's
work:

```bash
git branch --show-current
gh pr checkout <N>
git branch --show-current
```

If you are not on the PR branch after checkout, stop and investigate. Do not
edit or commit until you are on the correct branch.

### 4. Address actionable comments

Make code changes for every requested change. Keep each logical fix as a
separate commit when possible. Use named files/hunks only; never `git add .`.

After edits, run the relevant local checks for the repo (e.g. `cargo check`,
`cargo fmt --check`, `cargo nextest`, `pytest`). Do not push broken code just to
resolve a comment.

### 5. Push and resolve the threads

```bash
git push
```

Then, for each addressed thread, reply to explain what changed and mark the
thread resolved. Review threads on diff lines are resolved with the GraphQL API:

```bash
# Reply to the thread (optional but recommended)
gh pr review <N> --comment --body "Addressed in <commit-sha>: renamed x to width."

# Resolve the thread by its GraphQL node id
gh api graphql -F threadId='<thread-node-id>' -f query='
mutation($threadId:ID!) {
  resolveReviewThread(input:{threadId:$threadId}) { thread { isResolved } }
}'
```

General PR comments (not attached to a diff line) cannot be marked resolved the
same way — reply to them and ask the reviewer to resolve or dismiss them.

Re-run the inspection query from step 1 and confirm **zero unresolved threads**.

### 6. Re-inspect right before merging

Review threads, CI status, and the PR head can change between your first
inspection and the merge. Right before merging:

```bash
gh pr view <N> --json reviewDecision,mergeStateStatus,mergeable,headRefOid
```

- `reviewDecision` must be `APPROVED`. If it is still `CHANGES_REQUESTED`, stop —
  resolving threads is not enough. The reviewer must submit a new approving
  review or dismiss the request before GitHub allows merge.
- Re-run the unresolved-thread query from step 1 and confirm zero unresolved
  threads again.
- If anything changed, go back to step 1.

### 7. Merge only when clean

```bash
[ "$(gh pr view <N> --json isDraft --jq .isDraft)" = true ] || gh pr ready <N> --undo
bash scripts/premerge_gate.sh --pr <N>
```

The guarded command rechecks review-ready local state against the exact current
base/head, executes the required gates, marks the PR ready, and performs the
pinned regular merge. Do not use a direct merge fallback (Issue #11056).

Do not bypass a `CHANGES_REQUESTED` state by force-merging or using admin
override.

## Anti-patterns

- Merging after fixing code but without resolving the GitHub threads.
- Resolving a thread without making the change or replying.
- Treating an approving review as a guarantee that all threads are resolved.
- Squashing when the repo policy requires a regular merge.
- Skipping local checks because the change was "just a rename."
- Merging without `--match-head-commit` when the PR head could move.
- Switching branches blindly in a shared worktree.

## Red flags — STOP and re-read this skill

- "I fixed it in the code, so the comment is basically resolved."
- "The reviewer approved, so the open comments don't matter."
- "I'll merge now and resolve the thread afterwards."
- "This is just a nit, no need to reply before resolving."
- "I checked the threads a minute ago, so they must still be clean."
- "The PR head won't move in the seconds before I click merge."

## Quick reference

| Goal | Command |
|------|---------|
| View PR state | `gh pr view <N> --json reviewDecision,mergeStateStatus,reviews,comments` |
| List unresolved review threads | `gh api graphql` query for `reviewThreads { nodes { id isResolved ... } }` |
| Check out PR branch safely | `git branch --show-current && gh pr checkout <N> && git branch --show-current` |
| Reply to review | `gh pr review <N> --comment --body "..."` |
| Resolve a review thread | `gh api graphql` mutation `resolveReviewThread(input:{threadId:$id})` |
| Re-inspect before merge | `gh pr view <N> --json reviewDecision,mergeStateStatus,mergeable,headRefOid` |
| Merge (regular, pinned) | Ensure draft, then `bash scripts/premerge_gate.sh --pr <N>` |
