---
name: sjulia-logical-commits
description: >-
  Use when finishing a multi-file change in the SubsetJuliaVM (sjulia) repo and
  preparing commits, or when an existing branch has accumulated mixed concerns
  that should be separated before a PR.
---

# Logical Commits (sjulia)

Commit work as a sequence of logical, self-contained commits. Each commit is
one coherent change — a reviewer should be able to understand it in isolation,
build it, and run its tests without needing the next commit.

## Hard rules

- **One logical change per commit.** Never mix unrelated concerns (a bug fix
  + an unrelated refactor + a doc tweak in one commit is forbidden).
- **Keep each commit buildable.** `cargo build --release -p subset_julia_vm --bin sjulia
  --features repl` and the touched category's
  `cargo nextest run --release --test fixture_tests <category>::` should pass
  at every commit when feasible. If a commit is intentionally intermediate,
  say so in the body and make the next commit complete it.
- **Stage only named files or hunks.** No `git add .` / `git add -A`. Stage
  the exact files/hunks that belong to the logical unit.
- **Check your branch before EVERY commit.** `git branch --show-current` —
  shared worktrees get switched by other agents; a commit that lands on `main`
  or someone else's branch is an incident. Unexpected branch → STOP and return
  to your own branch first.
- **Never `git stash`.** The stash is repo-global across sessions/worktrees.
  To isolate files from another ref use `git checkout <ref> -- <file>`; to
  park WIP, commit it on a temporary branch.
- **Never discard others' work.** No destructive git commands
  (`reset --hard`, `clean -fdx`, branch deletion, force-push, history rewrite)
  on files you didn't modify, and never without explicit user approval.
- **Do not commit secrets** (`.env`, credentials, cache bins under `target/`).
  Warn if asked to.

## What is a logical unit in sjulia

A logical unit groups everything that belongs to ONE change. Typical units:

| Unit | Files it groups together |
|------|--------------------------|
| Bug fix | Rust/Julia source fix **+** regression fixture `.jl` **+** `manifest.toml` entry **+** `docs/vm` DONE/STATUS line |
| New Julia function | `src/julia/base|stdlib/<file>.jl` **+** fixture `.jl` + `manifest.toml` entry |
| New type/literal | All `Literal`/`ConcreteType`/`ArrayElementType` pipeline files from the matching `docs/vm/CHECKLISTS.md` section, in one commit, plus fixtures |
| Workaround | workaround comment (Rust `// Workaround: (Issue #NNNN)` or Julia `# Workaround:`) **+** `docs/vm/WORKAROUNDS.md` entry, in the same commit as the code it documents |
| Audit script | new `scripts/check_*.sh` **+** `docs/vm/CODE_AUDITS.md` entry |
| Memory notes | `memory/**/*.md` + `memory/MEMORY.md` index line (post-mortem output) |
| Pure docs | `docs/vm/*.md` only — separate from code commits |

Split when two units are independent; keep them together when one is
meaningless without the other (e.g. a fixture without its `manifest.toml`
entry, or a workaround comment without its `WORKAROUNDS.md` registration).

## Ordering

Order commits foundations-first:

1. Foundation / refactors that later commits depend on.
2. The feature/fix itself.
3. Its tests/fixtures.
4. Docs (`docs/vm/STATUS.md`, `DONE.md`, `UNIMPLEMENTED.md`) and audit wiring.

If a fix and its regression test must be one commit (so the tree is never in a
"fixed but untested" state), keep them together — do not split test from fix.

## Workflow

### 1. Inspect the current change set

```bash
git branch --show-current    # your branch? (see Hard rules)
git status
git diff --stat
git diff
```

Identify which files/hunks belong to which logical unit.

### 2. Propose the commit plan

Before committing, list the planned commits in order, each with:

- a draft one-line subject (imperative mood, ≤72 chars)
- the files/hunks it stages
- the WHY in one sentence
- the Issue number it references (if any)

Get user approval for the plan before executing when the split is non-trivial.

### 3. Execute — stage named files/hunks only

For each logical unit:

```bash
git branch --show-current      # re-check immediately before each commit
git add <file1> <file2> ...    # whole files belonging to this unit
git add -p <file>              # or specific hunks when a file spans units
```

### 4. Write the commit message

Subject: imperative mood, ≤72 chars, no trailing period. Use a conventional
prefix (`fix:`, `feat:`, `test:`, `docs:`, `chore:`) only if the repo already
does — otherwise plain imperative.

Body (wrapped at ~72 cols):

- **WHY** the change is needed (the diff already shows WHAT).
- The root cause or context, not a restatement of the diff.
- The Issue link: `Issue #NNNN` (and `Fixes #NNNN` on the final commit if it
  closes the issue).
- Anything non-obvious a reviewer would need (a trade-off, a skipped case, a
  follow-up).

Use a HEREDOC so the body renders correctly:

```bash
git commit -m "$(cat <<'EOF'
Add mixed-type Real==Complex methods to stop promote-fallback recursion

The generic ==(x::Number, y::Number) promote fallback only terminates when
promote widens to a type with a more-specific method. Real==Complex had no
specific method, so it re-dispatched on the unchanged pair forever and OOM'd
the host under the full suite (Issue #5966). Mirror upstream's mixed-type
methods so promote never reaches the fallback for these pairs.

Issue #5966
EOF
)"
```

### 5. Verify before moving on

After each commit (or at least after the final one):

```bash
cargo build --release -p subset_julia_vm --bin sjulia --features repl
timeout 1800 cargo nextest run --release --test fixture_tests <category>::
```

Finish with the full suite before opening the PR — never `| tail`
(it hides which test failed; numeric bugs are dispatch/cache/HashMap-seed
dependent and only surface in the full run):

```bash
timeout 1800 cargo nextest run --release
```

### 6. Confirm

```bash
git log --oneline -<n>      # the commit sequence reads as a story
git status                  # nothing left uncommitted that belongs to the PR
```

Each subject should make sense on its own; reading `git log --oneline` from
bottom to top should tell the reviewer what happened and why.

## Anti-patterns

- ❌ One giant "wip" / "implement feature X" commit bundling fix + tests +
  unrelated refactor + docs.
- ❌ Splitting a fix from its regression test so an intermediate commit is
  "fixed but untested".
- ❌ Commit message body that paraphrases the diff ("add function foo that
  calls bar") instead of explaining WHY.
- ❌ `git add .` / `git add -A` — stage only what belongs to the unit.
- ❌ Committing without re-checking `git branch --show-current`.
- ❌ `git stash` in a shared repo — park WIP on a branch commit instead.
- ❌ Forgetting the Issue link on a workaround/bug-fix commit.
- ❌ Committing a workaround comment without its `WORKAROUNDS.md` entry in the
  same commit (the workaround check scripts would fail mid-history).
- ❌ Destructive git ops on files you didn't modify.

## How this fits with the other sjulia skills

- `sjulia-report-gap` produces the Issue you reference in fix commits.
- `sjulia-bug-prevention` produces the prevention commit (audit script /
  checklist / coverage test) — a separate logical commit from the fix.
- `sjulia-finish-branch` / `create-pr` cover the PR + regular-merge flow after
  commits land; `sjulia-postmortem` runs after the merge.
