---
name: sjulia-report-gap
description: >-
  Use the moment you discover a construct that runs in upstream `julia` but
  does not run correctly in sjulia — a parse error, an "unsupported"/"not
  implemented" runtime error, a MethodError on otherwise-valid syntax, a
  crash, or wrong output — even if the gap is incidental to your current task
  and even if you could easily route around it.
---

# Report Upstream-vs-sjulia Gaps Immediately

This skill enforces the **Unsupported-Feature / Bug Discovery Rule** from
`AGENTS.md`: when you hit a construct that works in upstream
`julia` but fails in sjulia, the FIRST action is to file an Issue — not to
work around it.

## The rule (non-negotiable)

If a construct **runs in upstream `julia`** but **does not work in sjulia**,
you must:

1. **STOP** current implementation work on that construct.
2. **Do NOT** apply an ad-hoc workaround, special case, or package-specific
   shortcut. Do NOT silently edit code (or delete the failing line/test) to
   make your current task pass.
3. **File a GitHub Issue now** with the correct label (`unsupported-feature`
   or `bug`) and a minimal MWE + julia-vs-sjulia output table — after a
   duplicate check.
4. **Only then**, if your current task is blocked, add a workaround that
   references the Issue number — and register it in
   `docs/vm/WORKAROUNDS.md`.
5. Reference the Issue number in workaround comments, tests, and the PR.

This applies **even if you found the gap incidentally** while doing something
else. Routing around it silently is explicitly forbidden.

## Label decision

| sjulia behavior | Label |
|-----------------|-------|
| **Cannot run** the construct: parse error, "unsupported"/"not implemented" runtime error, MethodError on otherwise-valid syntax, or any other refusal to execute | `unsupported-feature` |
| **Runs but produces wrong output**, or crashes, or hits an existing sjulia error during work | `bug` |

When unsure, prefer `unsupported-feature` if sjulia refuses to execute, and
`bug` if sjulia executes but the result is wrong or it crashes.

## Step-by-step

### 1. Confirm the gap with a minimal reproducer

Reduce the failing case to the smallest MWE that still reproduces. Strip
everything unrelated. The MWE must run under upstream `julia` (proving it is
valid Julia) and fail under sjulia.

```bash
julia --startup-file=no --history-file=no <mwe.jl>   # must succeed
cargo build --release -p subset_julia_vm --bin sjulia --features repl    # refresh if base/ changed
timeout 180 target/release/sjulia <mwe.jl>            # must fail / differ
```

Capture the exact output of both. If `julia` is not on PATH, note that in the
Issue body and still file it — do not block on parity tooling.

### 2. Check for an existing Issue

```bash
gh issue list --state all --search "<construct keywords>" --limit 30
```

An existing Issue covers it → do not file a duplicate. Reference the existing
number in your workaround comment / tests / PR instead, and continue at step 4.
("I vaguely remember an issue" is not a check — run the search.)

### 3. File the Issue now

```bash
gh issue create --title "<short description>" \
  --label "unsupported-feature" \
  --body "$(cat <<'EOF'
## MWE

```julia
# minimal reproducer — runs in upstream julia, fails in sjulia
```

## Output

| Interpreter | Result |
|-------------|--------|
| `julia`     | <expected output / pass> |
| `sjulia`    | <parse error / unsupported error / MethodError / wrong output / crash> |

## Context
- Found while: <what you were doing, e.g. "adding fixture for X">
- sjulia build: `cargo build --release -p subset_julia_vm --bin sjulia --features repl`
EOF
)"
```

Use `--label "bug"` instead when sjulia runs but produces wrong output or
crashes.

### 4. Only then, decide on next action

- **If your current task is NOT blocked by the gap:** leave it for the Issue
  to track. Continue your original work; reference the Issue number if your
  work touches nearby code (e.g. in the fixture comment for a dropped test
  line).
- **If your current task IS blocked:**
  1. Add a workaround comment referencing the Issue:
     - Rust: `// Workaround: ... (Issue #NNNN)`
     - Julia: `# Workaround: ... (Issue #NNNN)`
  2. Add an entry to `docs/vm/WORKAROUNDS.md` (Issue #2843). For the full
     section + Summary Table workflow, apply `sjulia-document-workaround`.
  3. Run both sync scripts:
     ```bash
     bash scripts/check_workarounds_documented.sh
     bash scripts/check_workarounds_sync.sh
     ```
  4. Reference the Issue number in the PR body.

### 5. If you also plan to fix it

Filing the Issue first does not prevent you from fixing it in the same PR.
But the Issue must exist before the fix lands, and the fix must include a
regression test. To turn the fix into repo-wide prevention, apply
`sjulia-bug-prevention` after the fix.

## Forbidden actions

- ❌ Editing code (or deleting a failing test line) to silently make your task
  pass without an Issue.
- ❌ Adding an ad-hoc special case or package-specific shortcut when a
  structural upstream-compatible path is required.
- ❌ Filing the Issue *after* the workaround/fix. The Issue comes first.
- ❌ Using a workaround comment without registering it in
  `docs/vm/WORKAROUNDS.md` and running both check scripts.
- ❌ Skipping the Issue because the gap is "incidental" to your current task.
- ❌ Skipping the duplicate search because you "remember" an Issue exists.

## Quick reference: label + first action

```
upstream julia runs it, sjulia cannot run it        →  unsupported-feature  →  dedup check → file Issue
upstream julia runs it, sjulia runs wrong/crashes   →  bug                  →  dedup check → file Issue
sjulia runs it correctly but you dislike the shape  →  (no gap)             →  not this skill
```
