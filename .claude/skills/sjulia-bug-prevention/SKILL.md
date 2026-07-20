---
name: sjulia-bug-prevention
description: >-
  Use after fixing (or while fixing) a bug in SubsetJuliaVM (sjulia) — wrong
  output, crash, or an existing-error class — when the fix should become
  durable, repo-wide prevention, or when the user asks for a prevention Issue
  or 再発防止.
---

# Bug → Fix → Prevent (sjulia)

Workflow for turning a one-off bug fix into durable prevention across the
SubsetJuliaVM repo. This complements the `sjulia-dev` skill: that skill covers
the upstream-first implementation and PR flow; this skill focuses on
**extracting preventive knowledge from a fix and filing it as an Issue**.

## When to use

Apply after you have fixed (or are about to fix) a bug in sjulia. A "bug" here
means sjulia **ran but produced wrong output**, or you hit an existing sjulia
error / crash / compatibility gap during work. (If sjulia **cannot run** a
construct that upstream `julia` runs, that is `unsupported-feature`, not a bug
— use the `sjulia-dev` issue workflow instead.)

Per `AGENTS.md`: create the `bug` Issue **before** adding the workaround or
fix, then reference the Issue number in tests and PR. This skill adds a
**second, preventive Issue** (or expands the same one) that captures the
knowledge gained from the fix.

## Workflow

### 1. File the bug Issue first (issue-driven rule)

```bash
gh issue create --title "<short bug description>" \
  --label "bug" \
  --body "$(cat <<'EOF'
## MWE
```julia
# minimal reproducer
```

## Expected (upstream julia)
<output>

## Actual (sjulia)
<output>
EOF
)"
```

Reference this Issue number in the fix commit, regression test, and PR.

### 2. Fix the bug

Follow the upstream-first principle from `AGENTS.md`: consult
`./julia` for the correct semantics, reproduce the fix at the matching path
under `subset_julia_vm/src/julia/` (Pure Julia first) or in the Rust VM
handler only if no upstream-shaped path exists. Add a regression test in the
matching fixture category
(`subset_julia_vm/tests/fixtures/<category>/`) — verify with
`julia path/to/test.jl` and
`bash scripts/fixture_julia_parity.sh <fixture.jl>`.

### 3. Extract preventive knowledge

Answer these questions explicitly before drafting the preventive Issue:

- **Root cause:** what exactly allowed the bug? (e.g. a promote-fallback
  recursion, a missing mixed-type method, an early-route that swallowed a
  type, a `pop_i64` truncation, an exhaustive match missing a new variant).
- **Why did existing tests miss it?** (untested type pair, only inline
  constructor form tested but not variable-bound form, no `typeof()` assertion,
  category gap, no audit script).
- **Regression test:** which fixture + assertion now guards this exact case?
  Include both inline and variable-bound `typeof()` forms when relevant (see
  `docs/vm/CHECKLISTS.md` "New Primitive Type" section).
- **Blast radius:** what other call sites / types / operators share the same
  shape and could harbour the same bug? Enumerate them with
  `codegraph explore "<symbol>"` (shell; or the `codegraph_explore` MCP tool),
  falling back to `rg` when the repo is not indexed.
- **Prevention mechanism:** pick the strongest viable one (see below).

### 4. File the prevention Issue

Draft a second Issue (or a clearly-marked "Prevention" section in the bug
Issue) with the extracted knowledge:

```markdown
## Prevention: <bug class name>

### Root cause
<one paragraph>

### Why existing tests missed it
<bullets>

### Regression test
- `subset_julia_vm/tests/fixtures/<category>/<file>.jl` — `<testset name>`

### Blast radius (same-shape call sites)
- <file:line> — <symbol>
- ...

### Proposed prevention
- [ ] <audit script / checklist / lint / fixture category>
- [ ] Owner / follow-up Issue links
```

Choose a label: `bug` for a wrong-output regression class, or `unsupported-feature` if the preventive work surfaces a construct sjulia cannot run yet. Add `documentation` if the main output is a checklist/doc update.

### 5. Prevention mechanism — pick the strongest viable

In order of preference (strongest first):

1. **Audit script** (`scripts/check_*.sh`) — a deterministic, CI-registered
   gate that fails if the bug class reappears. Follow the "Adding a New Audit
   Script" checklist in `docs/vm/CODE_AUDITS.md`; ensure `bash3` compatibility
   (`bash scripts/check_audit_scripts_bash3_compat.sh`). Every `check_*.sh`
   must be referenced in `docs/vm/CODE_AUDITS.md`. CAUTION: do NOT edit
   `.github/workflows/*.yml` in the same PR — push tokens here usually lack
   the `workflow` scope and the push will be rejected; register the script in
   `CODE_AUDITS.md` and leave CI wiring to the user (note it in the PR body).
2. **Implementation checklist** entry in `docs/vm/CHECKLISTS.md` under the
   matching section (new type, new literal, new operator, AoT op, …) so future
   contributors don't repeat the omission.
3. **Exhaustive-match coverage test** — when the bug was a missing enum
   variant, add a `test_all_<Enum>_variants_*` coverage test (see
   `ConcreteType` / `ArrayElementType` / `IOKind` checklists) so adding a new
   variant without updating all match sites fails the build.
4. **Fixture category expansion** — add the missing type pair / form to the
   relevant fixture category and run
   `bash scripts/check_fixture_test_names.sh`.
5. **Clippy lint / pattern note** — if a new lint pattern is introduced,
   update Code Audits (Issue #3292). `cargo clippy --all-targets -- -D warnings`
   must pass (zero warnings).

### 6. Wire it in and verify

```bash
# If you added an audit script:
bash scripts/check_<your_script>.sh
bash scripts/check_audit_scripts_bash3_compat.sh

# If you added/changed fixtures:
bash scripts/check_fixture_test_names.sh
bash scripts/check_fixture_chunk_size.sh

# If you added a workaround comment:
bash scripts/check_workarounds_documented.sh
bash scripts/check_workarounds_sync.sh

# Clippy gate (zero warnings required)
cargo clippy --all-targets -- -D warnings

# Category + full suite
timeout 1800 cargo nextest run --release --test fixture_tests <category>::
timeout 1800 cargo nextest run --release   # never `| tail`
```

### 7. Update docs and PR

- Update `docs/vm/STATUS.md`, `DONE.md`, `UNIMPLEMENTED.md` using the
  dated-header policy (Issue #3760) and yearly archive policy (Issue #6341).
- Link both the bug Issue and the prevention Issue in the PR body.
- Regular merge, never squash; keep the PR draft until the lead runs
  `scripts/premerge_gate.sh --pr <N>`.

## Anti-patterns

- Do NOT fix-and-forget: a fix without a regression test + prevention Issue
  means the same bug class will recur in a parallel merge.
- Do NOT skip the blast-radius step: numeric/operator bugs are
  dispatch/cache/HashMap-seed dependent and can pass targeted tests yet OOM
  the full suite (Issue #5966). Always finish with a full
  `cargo nextest run --release`.
- Do NOT file the prevention Issue instead of the bug Issue — the bug Issue
  must exist first (issue-driven rule), with the prevention knowledge attached
  or linked.
- Do NOT introduce a workaround comment without registering it in
  `docs/vm/WORKAROUNDS.md` and running both workaround check scripts.
